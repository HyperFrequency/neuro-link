//! P17 — End-to-end integration test for `nlr_pdf_ingest`.
//!
//! Skips gracefully when `poppler-utils` (pdftotext / pdfinfo / pdfimages /
//! pdftoppm) is not installed. On CI we install poppler-utils explicitly so
//! this test actually runs; locally it is a soft-skip.
//!
//! Flow:
//!   1. Write a minimal hand-crafted 1-page PDF to a tempdir.
//!   2. Spawn the MCP server (stdio) with a scratch NLR_ROOT.
//!   3. Call `nlr_pdf_ingest` via JSON-RPC over stdio.
//!   4. Assert:
//!      - 01-sorted/<domain>/<slug>.md exists and is non-empty
//!      - 00-raw/<slug>/metadata.json exists with pages == 1
//!      - Either the markdown has an attachment link OR the `_attachments`
//!        directory has at least one PNG/PPM/JPG (soft — pdfimages may extract
//!        nothing from a text-only PDF).

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn nlr_binary() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("target");
    // Debug build for tests.
    path.push("debug");
    path.push("neuro-link");
    path
}

fn cmd_exists(bin: &str) -> bool {
    Command::new(bin)
        .arg("-v")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success() || s.code().is_some())
        .unwrap_or(false)
}

struct McpClient {
    child: std::process::Child,
    stdin: Option<std::process::ChildStdin>,
    reader: BufReader<std::process::ChildStdout>,
    next_id: u64,
}

impl McpClient {
    fn spawn(nlr_root: &Path) -> Self {
        let binary = nlr_binary();
        if !binary.exists() {
            panic!(
                "neuro-link binary not found at {:?}. Run `cargo build` first.",
                binary
            );
        }

        let mut child = Command::new(&binary)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .env("NLR_ROOT", nlr_root.to_str().unwrap())
            .spawn()
            .expect("failed to spawn nlr");

        let stdin = child.stdin.take().unwrap();
        let reader = BufReader::new(child.stdout.take().unwrap());

        let mut client = McpClient {
            child,
            stdin: Some(stdin),
            reader,
            next_id: 0,
        };

        let _ = client.send("initialize", None);
        client
    }

    fn send(&mut self, method: &str, params: Option<Value>) -> Value {
        self.next_id += 1;
        let mut msg = json!({
            "jsonrpc": "2.0",
            "method": method,
            "id": self.next_id,
        });
        if let Some(p) = params {
            msg["params"] = p;
        }
        let line = format!("{}\n", serde_json::to_string(&msg).unwrap());
        let stdin = self.stdin.as_mut().unwrap();
        stdin.write_all(line.as_bytes()).unwrap();
        stdin.flush().unwrap();

        let mut response = String::new();
        self.reader.read_line(&mut response).unwrap();
        serde_json::from_str(&response).unwrap()
    }

    fn call_tool(&mut self, name: &str, arguments: Option<Value>) -> Value {
        let mut params = json!({"name": name});
        if let Some(args) = arguments {
            params["arguments"] = args;
        }
        self.send("tools/call", Some(params))
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        drop(self.stdin.take());
        let _ = self.child.wait();
    }
}

fn setup_nlr_root(tmp: &tempfile::TempDir) {
    let root = tmp.path();
    let dirs = [
        "00-raw",
        "01-sorted/docs",
        "01-sorted/math",
        "01-sorted/ml-nn",
        "01-sorted/quant",
        "01-sorted/scientific-computing",
        "01-sorted/software-engineering",
        "02-KB-main",
        "03-ontology-main/domain",
        "04-KB-agents-workflows",
        "05-insights-gaps",
        "06-progress-reports",
        "07-neuro-link-task",
        "08-code-docs",
        "09-business-docs",
        "config",
        "state",
        "secrets",
    ];
    for d in &dirs {
        std::fs::create_dir_all(root.join(d)).unwrap();
    }
    std::fs::write(root.join("CLAUDE.md"), "# nlr test scaffold\n").unwrap();
    std::fs::write(
        root.join("02-KB-main/schema.md"),
        "---\ntitle: Schema\n---\n# Wiki Schema\n",
    )
    .unwrap();
    std::fs::write(root.join("02-KB-main/index.md"), "# Index\n").unwrap();
    std::fs::write(root.join("02-KB-main/log.md"), "# Mutation Log\n").unwrap();
    std::fs::write(
        root.join("config/neuro-link.md"),
        "---\nversion: 1\n---\n# Master Config\n",
    )
    .unwrap();
    std::fs::write(
        root.join("state/heartbeat.json"),
        r#"{"status":"ok","last_check":"2026-01-01T00:00:00Z","errors":[]}"#,
    )
    .unwrap();
    std::fs::write(root.join("state/session_log.jsonl"), "").unwrap();
    std::fs::write(root.join("state/score_history.jsonl"), "").unwrap();
    std::fs::write(root.join("state/deviation_log.jsonl"), "").unwrap();
}

/// Build a valid minimal 1-page PDF 1.4 at runtime. Computes xref byte
/// offsets correctly so strict poppler builds accept the fixture. Contains
/// one Tj text string so pdftotext produces non-empty output.
fn build_minimal_pdf() -> Vec<u8> {
    let content_stream = b"BT\n/F1 24 Tf\n72 720 Td\n(Hello from P17 neuro-link test) Tj\nET\n";
    let len_content = content_stream.len();
    let objs: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Count 1 /Kids [3 0 R] >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>".to_vec(),
        {
            let mut v = format!("<< /Length {len_content} >>\nstream\n").into_bytes();
            v.extend_from_slice(content_stream);
            v.extend_from_slice(b"endstream");
            v
        },
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec(),
    ];

    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n");

    let mut offsets: Vec<usize> = Vec::with_capacity(objs.len() + 1);
    offsets.push(0); // object 0 is always the free-list head
    for (i, body) in objs.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    }

    let xref_pos = out.len();
    out.extend_from_slice(b"xref\n");
    out.extend_from_slice(format!("0 {}\n", objs.len() + 1).as_bytes());
    out.extend_from_slice(b"0000000000 65535 f \n");
    for off in offsets.iter().skip(1) {
        out.extend_from_slice(format!("{:010} 00000 n \n", off).as_bytes());
    }
    out.extend_from_slice(b"trailer\n");
    out.extend_from_slice(format!("<< /Size {} /Root 1 0 R >>\n", objs.len() + 1).as_bytes());
    out.extend_from_slice(b"startxref\n");
    out.extend_from_slice(format!("{}\n", xref_pos).as_bytes());
    out.extend_from_slice(b"%%EOF\n");
    out
}

#[test]
fn pdf_ingest_end_to_end() {
    if !cmd_exists("pdftotext") || !cmd_exists("pdfinfo") {
        println!("skipping — poppler-utils not installed (pdftotext/pdfinfo missing)");
        return;
    }

    // 1. Scratch NLR_ROOT.
    let tmp = tempfile::tempdir().unwrap();
    setup_nlr_root(&tmp);
    let root = tmp.path();

    // 2. Write minimal PDF to a second tempdir (keeps it outside NLR_ROOT).
    let pdf_dir = tempfile::tempdir().unwrap();
    let pdf_path = pdf_dir.path().join("hello-p17.pdf");
    std::fs::write(&pdf_path, build_minimal_pdf()).unwrap();

    // Sanity: pdftotext can parse this PDF. If not, skip — the fixture is
    // not the system under test.
    let pdftotext_ok = Command::new("pdftotext")
        .arg(&pdf_path)
        .arg("-")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !pdftotext_ok {
        println!("skipping — minimal PDF fixture rejected by pdftotext on this poppler version");
        return;
    }

    // 3. Call nlr_pdf_ingest via MCP stdio.
    let mut client = McpClient::spawn(root);
    let resp = client.call_tool(
        "nlr_pdf_ingest",
        Some(json!({ "path": pdf_path.to_str().unwrap() })),
    );

    // The tool returns its payload as JSON text inside content[0].text.
    let text = resp["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| {
            panic!(
                "nlr_pdf_ingest returned no text content. Full response: {}",
                resp
            )
        });
    let payload: Value = serde_json::from_str(text)
        .unwrap_or_else(|e| panic!("failed to parse tool payload as JSON: {e}\nraw: {text}"));

    assert_eq!(payload["status"], "ingested", "tool should report ingested; got: {payload}");
    let slug = payload["slug"].as_str().expect("slug missing").to_string();
    let domain = payload["domain"].as_str().expect("domain missing").to_string();
    assert_eq!(payload["page_count"], 1, "expected single-page PDF");

    // 4a. Markdown written to 01-sorted/<domain>/<slug>.md.
    let md_path = root.join("01-sorted").join(&domain).join(format!("{slug}.md"));
    assert!(md_path.is_file(), "markdown not written at {md_path:?}");
    let md = std::fs::read_to_string(&md_path).unwrap();
    assert!(!md.trim().is_empty(), "markdown is empty");

    // 4b. metadata.json exists with pages: 1.
    let meta_path = root.join("00-raw").join(&slug).join("metadata.json");
    assert!(meta_path.is_file(), "metadata.json missing at {meta_path:?}");
    let meta: Value = serde_json::from_str(&std::fs::read_to_string(&meta_path).unwrap()).unwrap();
    assert_eq!(meta["pages"], 1, "metadata.json pages should be 1, got: {meta}");

    // 4c. Attachments: either an attachment link in the markdown, or at least
    // one file under _attachments/<slug>/. A text-only minimal PDF may have
    // zero extracted images, so we accept either signal (including "no
    // attachments needed because the PDF carried none") and just check the
    // attachments array is present + well-formed in the tool payload.
    let attachments = payload["attachments"]
        .as_array()
        .expect("attachments array missing from tool payload");
    let att_dir = root.join("_attachments").join(&slug);
    let att_dir_has_files = att_dir.is_dir()
        && std::fs::read_dir(&att_dir)
            .map(|it| it.count())
            .unwrap_or(0)
            > 0;
    // Soft check: we allow the empty case because pdfimages can legitimately
    // return nothing for a text-only minimal fixture. At minimum, verify the
    // tool reported a JSON array (even if empty) and that if it reported any
    // attachments, they actually landed on disk.
    if !attachments.is_empty() {
        assert!(
            att_dir_has_files,
            "tool reported {} attachments but _attachments/{slug} is empty",
            attachments.len()
        );
    }
}
