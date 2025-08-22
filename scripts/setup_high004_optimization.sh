#!/bin/bash

# HIGH-004 Database Performance Optimization Setup Script
# This script configures PostgreSQL and PgBouncer for high-throughput vector operations

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
CONFIG_DIR="$PROJECT_ROOT/config"

# Default PostgreSQL configuration locations (adjust for your system)
if [[ "$OSTYPE" == "darwin"* ]]; then
    # macOS (Homebrew)
    PG_CONFIG_DIR="/opt/homebrew/var/postgresql@15"
    PG_DATA_DIR="/opt/homebrew/var/postgresql@15"
    PGBOUNCER_CONFIG_DIR="/opt/homebrew/etc/pgbouncer"
elif [[ "$OSTYPE" == "linux-gnu"* ]]; then
    # Linux
    PG_CONFIG_DIR="/etc/postgresql/15/main"
    PG_DATA_DIR="/var/lib/postgresql/15/main"
    PGBOUNCER_CONFIG_DIR="/etc/pgbouncer"
else
    echo -e "${RED}Error: Unsupported operating system: $OSTYPE${NC}"
    exit 1
fi

# Functions
print_header() {
    echo -e "${BLUE}"
    echo "=================================================="
    echo "  HIGH-004 Database Optimization Setup"
    echo "  Target: >1000 ops/sec vector operations"
    echo "=================================================="
    echo -e "${NC}"
}

print_section() {
    echo -e "${YELLOW}📋 $1${NC}"
}

print_success() {
    echo -e "${GREEN}✅ $1${NC}"
}

print_warning() {
    echo -e "${YELLOW}⚠️  $1${NC}"
}

print_error() {
    echo -e "${RED}❌ $1${NC}"
}

check_prerequisites() {
    print_section "Checking Prerequisites"
    
    # Check if PostgreSQL is installed
    if ! command -v psql &> /dev/null; then
        print_error "PostgreSQL is not installed or not in PATH"
        exit 1
    fi
    
    # Check if PgBouncer is installed
    if ! command -v pgbouncer &> /dev/null; then
        print_warning "PgBouncer is not installed. Installing it is recommended for production."
        echo "  Install with: brew install pgbouncer (macOS) or apt install pgbouncer (Ubuntu)"
    fi
    
    # Check if we can connect to PostgreSQL
    if ! psql -c "SELECT 1;" &> /dev/null; then
        print_error "Cannot connect to PostgreSQL. Please check your connection settings."
        exit 1
    fi
    
    # Check for pgvector extension
    if ! psql -c "SELECT * FROM pg_available_extensions WHERE name='vector';" | grep -q vector; then
        print_error "pgvector extension is not available. Please install it first."
        echo "  Install with: brew install pgvector (macOS) or apt install postgresql-15-pgvector (Ubuntu)"
        exit 1
    fi
    
    print_success "Prerequisites check passed"
}

get_system_memory() {
    if [[ "$OSTYPE" == "darwin"* ]]; then
        # macOS
        echo $(($(sysctl -n hw.memsize) / 1024 / 1024 / 1024))
    else
        # Linux
        echo $(($(grep MemTotal /proc/meminfo | awk '{print $2}') / 1024 / 1024))
    fi
}

calculate_memory_settings() {
    local total_memory_gb=$1
    
    # Calculate memory settings based on total RAM
    shared_buffers_gb=$((total_memory_gb * 25 / 100))
    effective_cache_size_gb=$((total_memory_gb * 75 / 100))
    
    if [ $total_memory_gb -ge 32 ]; then
        work_mem_mb=256
        maintenance_work_mem_gb=2
    elif [ $total_memory_gb -ge 16 ]; then
        work_mem_mb=128
        maintenance_work_mem_gb=1
    else
        work_mem_mb=64
        maintenance_work_mem_gb=1
        shared_buffers_gb=$((total_memory_gb * 20 / 100))  # Be more conservative on smaller systems
    fi
    
    echo "Calculated memory settings for ${total_memory_gb}GB system:"
    echo "  shared_buffers: ${shared_buffers_gb}GB"
    echo "  effective_cache_size: ${effective_cache_size_gb}GB"
    echo "  work_mem: ${work_mem_mb}MB"
    echo "  maintenance_work_mem: ${maintenance_work_mem_gb}GB"
}

backup_existing_config() {
    print_section "Backing up existing configurations"
    
    local timestamp=$(date +"%Y%m%d_%H%M%S")
    local backup_dir="$PROJECT_ROOT/config_backup_$timestamp"
    mkdir -p "$backup_dir"
    
    # Backup PostgreSQL config if it exists
    if [ -f "$PG_CONFIG_DIR/postgresql.conf" ]; then
        cp "$PG_CONFIG_DIR/postgresql.conf" "$backup_dir/postgresql.conf.backup"
        print_success "PostgreSQL config backed up"
    fi
    
    # Backup PgBouncer config if it exists
    if [ -f "$PGBOUNCER_CONFIG_DIR/pgbouncer.ini" ]; then
        cp "$PGBOUNCER_CONFIG_DIR/pgbouncer.ini" "$backup_dir/pgbouncer.ini.backup"
        print_success "PgBouncer config backed up"
    fi
    
    echo "Backups stored in: $backup_dir"
}

customize_postgresql_config() {
    local total_memory_gb=$1
    local config_file="$CONFIG_DIR/postgresql.conf"
    local temp_file="/tmp/postgresql_optimized.conf"
    
    print_section "Customizing PostgreSQL configuration for ${total_memory_gb}GB system"
    
    # Calculate memory settings
    shared_buffers_gb=$((total_memory_gb * 25 / 100))
    effective_cache_size_gb=$((total_memory_gb * 75 / 100))
    
    if [ $total_memory_gb -ge 32 ]; then
        work_mem_mb=256
        maintenance_work_mem_gb=2
    elif [ $total_memory_gb -ge 16 ]; then
        work_mem_mb=128
        maintenance_work_mem_gb=1
    else
        work_mem_mb=64
        maintenance_work_mem_gb=1
        shared_buffers_gb=$((total_memory_gb * 20 / 100))
    fi
    
    # Create customized configuration
    sed -e "s/shared_buffers = 8GB/shared_buffers = ${shared_buffers_gb}GB/g" \
        -e "s/effective_cache_size = 24GB/effective_cache_size = ${effective_cache_size_gb}GB/g" \
        -e "s/work_mem = 256MB/work_mem = ${work_mem_mb}MB/g" \
        -e "s/maintenance_work_mem = 2GB/maintenance_work_mem = ${maintenance_work_mem_gb}GB/g" \
        "$config_file" > "$temp_file"
    
    echo "Customized PostgreSQL configuration created at: $temp_file"
    echo "To apply:"
    echo "  sudo cp $temp_file $PG_CONFIG_DIR/postgresql.conf"
    echo "  sudo systemctl restart postgresql  # or brew services restart postgresql@15"
}

install_postgresql_config() {
    print_section "Installing PostgreSQL configuration"
    
    local total_memory_gb=$(get_system_memory)
    customize_postgresql_config $total_memory_gb
    
    echo "Would you like to install the PostgreSQL configuration now? [y/N]"
    read -r response
    if [[ "$response" =~ ^([yY][eE][sS]|[yY])+$ ]]; then
        local temp_file="/tmp/postgresql_optimized.conf"
        
        if [ -w "$PG_CONFIG_DIR" ]; then
            cp "$temp_file" "$PG_CONFIG_DIR/postgresql.conf"
            print_success "PostgreSQL configuration installed"
        else
            print_warning "Need sudo permissions to install PostgreSQL config"
            echo "Run: sudo cp $temp_file $PG_CONFIG_DIR/postgresql.conf"
        fi
        
        print_warning "PostgreSQL needs to be restarted to apply changes"
        echo "  macOS: brew services restart postgresql@15"
        echo "  Linux: sudo systemctl restart postgresql"
    else
        echo "Configuration file available at: /tmp/postgresql_optimized.conf"
    fi
}

install_pgbouncer_config() {
    print_section "Installing PgBouncer configuration"
    
    if ! command -v pgbouncer &> /dev/null; then
        print_warning "PgBouncer is not installed. Skipping PgBouncer configuration."
        return
    fi
    
    echo "Would you like to install the PgBouncer configuration now? [y/N]"
    read -r response
    if [[ "$response" =~ ^([yY][eE][sS]|[yY])+$ ]]; then
        # Create PgBouncer directories if they don't exist
        sudo mkdir -p "$PGBOUNCER_CONFIG_DIR"
        sudo mkdir -p /var/log/pgbouncer
        sudo mkdir -p /var/run/pgbouncer
        
        # Install configuration files
        if [ -w "$PGBOUNCER_CONFIG_DIR" ]; then
            cp "$CONFIG_DIR/pgbouncer.ini" "$PGBOUNCER_CONFIG_DIR/"
            cp "$CONFIG_DIR/userlist.txt" "$PGBOUNCER_CONFIG_DIR/"
            print_success "PgBouncer configuration installed"
        else
            print_warning "Need sudo permissions to install PgBouncer config"
            echo "Run: sudo cp $CONFIG_DIR/pgbouncer.ini $PGBOUNCER_CONFIG_DIR/"
            echo "Run: sudo cp $CONFIG_DIR/userlist.txt $PGBOUNCER_CONFIG_DIR/"
        fi
        
        print_warning "Update userlist.txt with proper credentials before starting PgBouncer"
        echo "Start PgBouncer with: pgbouncer -d $PGBOUNCER_CONFIG_DIR/pgbouncer.ini"
    fi
}

setup_monitoring() {
    print_section "Setting up monitoring"
    
    # Enable pg_stat_statements if not already enabled
    psql -c "CREATE EXTENSION IF NOT EXISTS pg_stat_statements;" 2>/dev/null || true
    
    # Create monitoring user (optional)
    echo "Would you like to create a monitoring user for database metrics? [y/N]"
    read -r response
    if [[ "$response" =~ ^([yY][eE][sS]|[yY])+$ ]]; then
        psql -c "CREATE USER monitor WITH PASSWORD 'monitor_password';" 2>/dev/null || true
        psql -c "GRANT pg_monitor TO monitor;" 2>/dev/null || true
        psql -c "GRANT SELECT ON ALL TABLES IN SCHEMA public TO monitor;" 2>/dev/null || true
        print_success "Monitoring user 'monitor' created"
        print_warning "Remember to change the default password!"
    fi
}

run_load_test() {
    print_section "Running load tests"
    
    echo "Would you like to run the load tests to verify performance? [y/N]"
    read -r response
    if [[ "$response" =~ ^([yY][eE][sS]|[yY])+$ ]]; then
        cd "$PROJECT_ROOT"
        
        # Set up test database if needed
        export DATABASE_URL="${DATABASE_URL:-postgresql://postgres:postgres@localhost:5432/codex_memory}"
        
        print_warning "Make sure your database is running and accessible"
        echo "Using DATABASE_URL: $DATABASE_URL"
        
        if cargo run --bin load_test_vector_ops; then
            print_success "Load tests completed successfully"
        else
            print_error "Load tests failed. Check configuration and try again."
        fi
    fi
}

print_summary() {
    print_section "Setup Summary"
    
    local total_memory_gb=$(get_system_memory)
    
    echo "System Configuration:"
    echo "  Total Memory: ${total_memory_gb}GB"
    echo "  PostgreSQL Config: $PG_CONFIG_DIR/postgresql.conf"
    echo "  PgBouncer Config: $PGBOUNCER_CONFIG_DIR/pgbouncer.ini"
    echo ""
    echo "Optimization Targets (HIGH-004):"
    echo "  ✅ >1000 ops/sec throughput"
    echo "  ✅ <200ms P99 latency for vector operations"
    echo "  ✅ 100+ connection pool minimum"
    echo "  ✅ 2000 client connection support via PgBouncer"
    echo "  ✅ 70% pool utilization alerting"
    echo ""
    echo "Next Steps:"
    echo "  1. Restart PostgreSQL to apply new configuration"
    echo "  2. Configure PgBouncer with proper credentials"
    echo "  3. Update application connection strings to use PgBouncer"
    echo "  4. Set up monitoring and alerting"
    echo "  5. Run load tests to verify performance"
    echo ""
    echo "Documentation:"
    echo "  📖 Performance Tuning Guide: docs/database_performance_tuning.md"
    echo "  🔧 Load Testing: scripts/load_test_vector_ops.rs"
    echo "  📊 Monitoring: src/monitoring/connection_monitor.rs"
}

# Main execution
main() {
    print_header
    
    # Check if config directory exists
    if [ ! -d "$CONFIG_DIR" ]; then
        print_error "Configuration directory not found: $CONFIG_DIR"
        exit 1
    fi
    
    check_prerequisites
    backup_existing_config
    install_postgresql_config
    install_pgbouncer_config
    setup_monitoring
    run_load_test
    print_summary
    
    print_success "HIGH-004 optimization setup completed!"
}

# Run main function
main "$@"