#!/bin/bash

# Agentic Memory System Setup Script
# This script provides easy setup and management commands

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Default configuration
OLLAMA_HOST="${OLLAMA_HOST:-192.168.1.110:11434}"
DATABASE_URL="${DATABASE_URL:-postgresql://postgres:postgres@localhost:5432/codex_memory}"

print_banner() {
    echo -e "${BLUE}"
    echo "╔══════════════════════════════════════════════════════════════╗"
    echo "║                 Agentic Memory System Setup                 ║"
    echo "║              Advanced AI Memory Management                   ║"
    echo "╚══════════════════════════════════════════════════════════════╝"
    echo -e "${NC}"
}

log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

check_dependencies() {
    log_info "Checking system dependencies..."
    
    # Check if Rust/Cargo is installed
    if ! command -v cargo &> /dev/null; then
        log_error "Cargo not found. Please install Rust from https://rustup.rs/"
        exit 1
    fi
    
    # Check if PostgreSQL is available
    if ! command -v psql &> /dev/null; then
        log_warn "PostgreSQL client not found. Make sure PostgreSQL is accessible."
    fi
    
    # Check if Ollama is running
    if curl -s "http://${OLLAMA_HOST}/api/tags" > /dev/null 2>&1; then
        log_info "✅ Ollama is running at ${OLLAMA_HOST}"
    else
        log_warn "⚠️  Ollama not accessible at ${OLLAMA_HOST}"
        log_info "Please ensure Ollama is running or update OLLAMA_HOST environment variable"
    fi
    
    log_info "✅ Dependency check completed"
}

setup_environment() {
    log_info "Setting up environment configuration..."
    
    # Create .env file if it doesn't exist
    if [ ! -f .env ]; then
        log_info "Creating .env configuration file..."
        cargo run --bin codex-memory init-config
        log_info "✅ Configuration file created at .env.example"
        log_info "💡 Copy .env.example to .env and modify as needed"
    else
        log_info "✅ Environment file already exists"
    fi
}

setup_database() {
    log_info "Setting up database..."
    
    # Try to setup database using the CLI tool
    if cargo run --bin codex-memory database setup; then
        log_info "✅ Database setup completed"
    else
        log_error "❌ Database setup failed"
        log_info "💡 Please check your DATABASE_URL and PostgreSQL installation"
        return 1
    fi
}

setup_models() {
    log_info "Setting up embedding models..."
    
    # Check what models are available
    log_info "Checking available models..."
    cargo run --bin codex-memory models
    
    # Try to setup models automatically
    if cargo run --bin codex-memory setup --skip-database; then
        log_info "✅ Model setup completed"
    else
        log_warn "⚠️  Automatic model setup failed"
        log_info "💡 You may need to manually pull an embedding model:"
        log_info "   ollama pull nomic-embed-text"
        log_info "   ollama pull mxbai-embed-large"
    fi
}

run_health_check() {
    log_info "Running comprehensive health check..."
    
    if cargo run --bin codex-memory health --detailed; then
        log_info "✅ Health check passed"
        return 0
    else
        log_error "❌ Health check failed"
        return 1
    fi
}

build_project() {
    log_info "Building project..."
    
    if cargo build --release; then
        log_info "✅ Build completed successfully"
    else
        log_error "❌ Build failed"
        return 1
    fi
}

quick_setup() {
    log_info "🚀 Running quick setup..."
    
    check_dependencies
    setup_environment
    
    # Build first to ensure we have the binary
    build_project || return 1
    
    setup_database || return 1
    setup_models || return 1
    
    log_info "🎉 Quick setup completed!"
    log_info "💡 Run health check with: ./scripts/setup.sh health"
    log_info "💡 Start the server with: cargo run --bin codex-memory start"
}

show_help() {
    echo "Agentic Memory System Setup Script"
    echo ""
    echo "Usage: $0 [command]"
    echo ""
    echo "Commands:"
    echo "  quick        Run complete setup (default)"
    echo "  deps         Check system dependencies"
    echo "  env          Setup environment configuration"
    echo "  database     Setup database and migrations"
    echo "  models       Setup embedding models"
    echo "  health       Run health checks"
    echo "  build        Build the project"
    echo "  start        Start the server (after setup)"
    echo "  help         Show this help message"
    echo ""
    echo "Environment variables:"
    echo "  OLLAMA_HOST     Ollama server address (default: 192.168.1.110:11434)"
    echo "  DATABASE_URL    PostgreSQL connection URL"
    echo ""
    echo "Examples:"
    echo "  $0                    # Run quick setup"
    echo "  $0 health            # Check system health"
    echo "  $0 models            # Setup embedding models only"
    echo "  OLLAMA_HOST=localhost:11434 $0 quick"
}

# Main script logic
case "${1:-quick}" in
    "quick")
        print_banner
        quick_setup
        ;;
    "deps")
        print_banner
        check_dependencies
        ;;
    "env")
        print_banner
        setup_environment
        ;;
    "database")
        print_banner
        setup_database
        ;;
    "models")
        print_banner
        setup_models
        ;;
    "health")
        print_banner
        run_health_check
        ;;
    "build")
        print_banner
        build_project
        ;;
    "start")
        print_banner
        log_info "Starting Agentic Memory System..."
        cargo run --bin codex-memory start
        ;;
    "help"|"-h"|"--help")
        print_banner
        show_help
        ;;
    *)
        log_error "Unknown command: $1"
        show_help
        exit 1
        ;;
esac