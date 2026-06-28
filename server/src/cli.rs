use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "neuro-link", version, about = "neuro-link-recursive: unified MCP server + CLI")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Start MCP server (stdio JSON-RPC)
    Mcp,
    /// Initialize directory structure
    Init,
    /// System health check
    Status,
    /// Crawl + ingest a source
    Ingest {
        /// URL or file path to ingest
        sources: Vec<String>,
        /// Run parallel async crawl
        #[arg(long)]
        parallel: bool,
    },
    /// Placeholder: prints instructions to use wiki-curate skill
    Curate {
        /// Topic to curate
        topic: String,
    },
    /// Keyword search across wiki
    Search {
        /// Search query
        query: String,
        /// Max results
        #[arg(long, default_value_t = 10)]
        limit: usize,
    },
    /// Embed wiki into Qdrant
    Embed {
        /// Recreate collection before embedding
        #[arg(long)]
        recreate: bool,
    },
    /// Re-sort unclassified slugs in 00-raw/ through nlr_ingest_classify
    Resort,
    /// Drain the curation queue through `claude --print /wiki-curate <slug>`
    CurateBacklog {
        /// Maximum number of queue entries to process
        #[arg(long, default_value_t = 1)]
        limit: usize,
    },
    /// Run grading pipeline
    Grade {
        /// Grade session logs
        #[arg(long)]
        session: bool,
        /// Grade wiki quality
        #[arg(long)]
        wiki: bool,
    },
    /// Health check daemon
    Heartbeat {
        /// Check interval in seconds
        #[arg(long, default_value_t = 60)]
        interval: u64,
    },
    /// Brain scan (staleness, gaps, tasks)
    Scan,
    /// Task queue operations
    Tasks {
        #[command(subcommand)]
        action: Option<TaskAction>,
    },
    /// Read a config file
    Config {
        /// Config file name (without extension)
        name: Option<String>,
    },
    /// Start HTTP server (REST API + MCP over HTTP)
    Serve {
        /// Port to listen on
        #[arg(long, default_value_t = 8080)]
        port: u16,
        /// Bind address
        #[arg(long, default_value = "0.0.0.0")]
        bind: String,
        /// API token (use 'auto' to generate one)
        #[arg(long)]
        token: Option<String>,
        /// Allow unauthenticated access (local dev only, DANGEROUS over network)
        #[arg(long)]
        insecure_no_auth: bool,
        /// Start ngrok tunnel for remote access
        #[arg(long)]
        tunnel: bool,
        /// Custom ngrok domain (requires ngrok paid plan)
        #[arg(long)]
        tunnel_domain: Option<String>,
        /// Disable inbox watcher
        #[arg(long)]
        no_watch: bool,
    },
    /// Neo4j temporal graph operations
    Graph {
        #[command(subcommand)]
        action: GraphAction,
    },
    /// State machine workflow operations
    Workflow {
        #[command(subcommand)]
        action: WorkflowAction,
    },
    /// Recursive self-improvement worker (grade LLM logs + emit proposals)
    Worker {
        /// Interval between passes in minutes
        #[arg(long, default_value_t = 15)]
        interval: u64,
    },
    /// Service supervisor (monitors + restarts Qdrant/Neo4j/embedding server/ngrok)
    Supervise,
    /// Claude Code session tracking (parse/scan/list/export)
    Sessions {
        #[command(subcommand)]
        action: SessionsAction,
    },
    /// All-in-one: HTTP server + supervisor + worker + embedding server
    /// (equivalent to running serve, supervise, worker, and embedding-server.sh together)
    Start {
        /// Port to listen on
        #[arg(long, default_value_t = 8080)]
        port: u16,
        /// Bind address
        #[arg(long, default_value = "0.0.0.0")]
        bind: String,
        /// API token (use 'auto' to generate, or omit for existing token)
        #[arg(long)]
        token: Option<String>,
        /// Allow unauthenticated access
        #[arg(long)]
        insecure_no_auth: bool,
        /// Start ngrok tunnel
        #[arg(long)]
        tunnel: bool,
        /// Custom ngrok domain
        #[arg(long)]
        tunnel_domain: Option<String>,
        /// Worker interval in minutes
        #[arg(long, default_value_t = 15)]
        worker_interval: u64,
        /// Skip starting the embedding server
        #[arg(long)]
        no_embedding: bool,
        /// Skip starting the worker
        #[arg(long)]
        no_worker: bool,
        /// Skip starting the supervisor
        #[arg(long)]
        no_supervise: bool,
        /// Skip starting the session watcher
        #[arg(long)]
        no_sessions_watch: bool,
        /// Skip starting the inbox watcher
        #[arg(long)]
        no_watch: bool,
    },
}

#[derive(Subcommand)]
pub enum SessionsAction {
    /// Parse all Claude Code sessions and export to vault markdown
    Parse {
        /// Only sessions ended after this date (YYYY-MM-DD)
        #[arg(long)]
        since: Option<String>,
        /// Override vault path (default: read from config/neuro-link.md)
        #[arg(long)]
        vault: Option<std::path::PathBuf>,
    },
    /// List parsed sessions
    List {
        /// Look back N days (default 7)
        #[arg(long)]
        days: Option<u64>,
    },
    /// Run quality scan on recent sessions
    Scan {
        /// Look back N days (default 7)
        #[arg(long)]
        days: Option<u64>,
    },
    /// Export API passthrough logs to vault markdown
    ExportLlm {
        #[arg(long)]
        vault: Option<std::path::PathBuf>,
    },
    /// Watch session logs and continuously update vault markdown
    Watch {
        /// Override vault path
        #[arg(long)]
        vault: Option<std::path::PathBuf>,
        /// Poll interval in seconds (default 30)
        #[arg(long, default_value_t = 30)]
        interval: u64,
    },
}

#[derive(Subcommand)]
pub enum TaskAction {
    /// List tasks
    List {
        /// Filter by status
        #[arg(long, default_value = "all")]
        status: String,
    },
    /// Create a new task
    Create {
        /// Task title
        title: String,
        /// Task type (ingest, curate, scan, repair, report, ontology)
        #[arg(long, name = "type")]
        task_type: String,
        /// Priority 1-5
        #[arg(long, default_value_t = 3)]
        priority: u64,
    },
}

#[derive(Subcommand)]
pub enum GraphAction {
    /// Add a temporal fact triple
    AddFact {
        subject: String,
        predicate: String,
        object: String,
    },
    /// Query facts by topic
    Query {
        topic: String,
    },
}

#[derive(Subcommand)]
pub enum WorkflowAction {
    /// Create a new workflow
    Create {
        name: String,
    },
    /// Transition workflow to a new state
    Transition {
        id: String,
        state: String,
    },
    /// List all workflows
    List,
}
