#!/usr/bin/env bash
# Start the embedding server for Octen/Octen-Embedding-8B
# Uses llama.cpp's llama-server with F16 GGUF (unquantized precision)
# Falls back to Python sentence-transformers if llama.cpp unavailable

set -euo pipefail

PORT="${EMBEDDING_PORT:-8400}"
MODEL_DIR="${NLR_ROOT:-$(cd "$(dirname "$0")/.." && pwd)}/models"
MODEL_NAME="Octen-Embedding-8B"
GGUF_FILE="$MODEL_DIR/${MODEL_NAME}.f16.gguf"
HF_REPO="mradermacher/Octen-Embedding-8B-GGUF"

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

mkdir -p "$MODEL_DIR"

# ── Download model if missing ──
download_gguf() {
    echo -e "${YELLOW}Downloading ${MODEL_NAME} F16 GGUF...${NC}"

    # Try huggingface-cli first
    if command -v huggingface-cli &>/dev/null; then
        huggingface-cli download "$HF_REPO" \
            --include "*f16.gguf" \
            --local-dir "$MODEL_DIR" \
            --local-dir-use-symlinks False 2>/dev/null
        # Find the downloaded file
        local found=$(find "$MODEL_DIR" -name "*f16.gguf" -type f 2>/dev/null | head -1)
        if [ -n "$found" ] && [ "$found" != "$GGUF_FILE" ]; then
            mv "$found" "$GGUF_FILE"
        fi
    fi

    # Fallback: direct download
    if [ ! -f "$GGUF_FILE" ]; then
        echo -e "${YELLOW}Trying direct download...${NC}"
        # List files in the repo to find the F16 GGUF filename
        local base_url="https://huggingface.co/${HF_REPO}/resolve/main"
        # mradermacher naming: Octen-Embedding-8B.f16.gguf
        curl -L -o "$GGUF_FILE" \
            "${base_url}/Octen-Embedding-8B.f16.gguf" 2>/dev/null || true
    fi

    if [ ! -f "$GGUF_FILE" ] || [ ! -s "$GGUF_FILE" ]; then
        echo -e "${RED}Could not download F16 GGUF. Trying Q8_0 as fallback...${NC}"
        curl -L -o "$MODEL_DIR/${MODEL_NAME}.Q8_0.gguf" \
            "https://huggingface.co/${HF_REPO}/resolve/main/Octen-Embedding-8B.Q8_0.gguf" 2>/dev/null || true
        if [ -f "$MODEL_DIR/${MODEL_NAME}.Q8_0.gguf" ]; then
            GGUF_FILE="$MODEL_DIR/${MODEL_NAME}.Q8_0.gguf"
            echo -e "${YELLOW}Using Q8_0 quantization (near-lossless for embeddings)${NC}"
        fi
    fi
}

# ── Option 1: llama.cpp server ──
start_llama_server() {
    local llama_bin=""
    if command -v llama-server &>/dev/null; then
        llama_bin="llama-server"
    elif command -v llama-cpp-server &>/dev/null; then
        llama_bin="llama-cpp-server"
    elif [ -f "/usr/local/bin/llama-server" ]; then
        llama_bin="/usr/local/bin/llama-server"
    fi

    if [ -z "$llama_bin" ]; then
        return 1
    fi

    if [ ! -f "$GGUF_FILE" ]; then
        download_gguf
    fi

    if [ ! -f "$GGUF_FILE" ]; then
        echo -e "${RED}No GGUF model file found${NC}"
        return 1
    fi

    echo -e "${GREEN}Starting llama.cpp embedding server on port ${PORT}${NC}"
    echo "  Model: $GGUF_FILE"
    echo "  Endpoint: http://localhost:${PORT}/v1/embeddings"

    exec "$llama_bin" \
        --embeddings \
        --model "$GGUF_FILE" \
        --port "$PORT" \
        --host 0.0.0.0 \
        --ctx-size 4096 \
        --batch-size 512 \
        --threads "$(sysctl -n hw.logicalcpu 2>/dev/null || nproc 2>/dev/null || echo 4)"
}

# ── Option 2: Python sentence-transformers ──
start_python_server() {
    echo -e "${YELLOW}llama.cpp not found. Starting Python embedding server...${NC}"

    if ! command -v python3 &>/dev/null; then
        echo -e "${RED}Python3 not found. Install llama.cpp or Python3.${NC}"
        exit 1
    fi

    # Create venv if needed
    local venv_dir="$MODEL_DIR/.venv"
    if [ ! -d "$venv_dir" ]; then
        echo "Creating Python venv..."
        python3 -m venv "$venv_dir"
        "$venv_dir/bin/pip" install -q sentence-transformers flask
    fi

    echo -e "${GREEN}Starting Python embedding server on port ${PORT}${NC}"
    echo "  Model: Octen/Octen-Embedding-8B (downloading from HuggingFace)"
    echo "  Endpoint: http://localhost:${PORT}/v1/embeddings"

    exec "$venv_dir/bin/python3" - <<'PYSERVER'
import os, json, sys
from flask import Flask, request, jsonify
from sentence_transformers import SentenceTransformer

port = int(os.environ.get("EMBEDDING_PORT", "8400"))
model_name = "Octen/Octen-Embedding-8B"
print(f"Loading {model_name}...", flush=True)
model = SentenceTransformer(model_name, trust_remote_code=True)
print(f"Model loaded. Dimensions: {model.get_sentence_embedding_dimension()}", flush=True)

app = Flask(__name__)

@app.route("/v1/embeddings", methods=["POST"])
def embeddings():
    data = request.json
    texts = data.get("input", "")
    if isinstance(texts, str):
        texts = [texts]
    embeddings = model.encode(texts).tolist()
    return jsonify({
        "object": "list",
        "data": [{"object": "embedding", "index": i, "embedding": e} for i, e in enumerate(embeddings)],
        "model": model_name,
    })

@app.route("/health")
def health():
    return jsonify({"status": "ok", "model": model_name, "dimensions": model.get_sentence_embedding_dimension()})

app.run(host="0.0.0.0", port=port)
PYSERVER
}

# ── Main ──
echo "Octen/Octen-Embedding-8B embedding server"
echo "  Dimensions: 4096 (unquantized)"
echo ""

if start_llama_server 2>/dev/null; then
    :
else
    start_python_server
fi
