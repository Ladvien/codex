#!/usr/bin/env node

const { spawn } = require('child_process');
const path = require('path');

// Set environment variables
process.env.DATABASE_URL = process.env.DATABASE_URL || 'postgresql://codex_user:MZSfXiLr5uR3QYbRwv2vTzi22SvFkj4a@192.168.1.104:5432/codex_db';
process.env.EMBEDDING_PROVIDER = process.env.EMBEDDING_PROVIDER || 'ollama';
process.env.EMBEDDING_MODEL = process.env.EMBEDDING_MODEL || 'nomic-embed-text';
process.env.EMBEDDING_BASE_URL = process.env.OLLAMA_BASE_URL || 'http://192.168.1.110:11434';
process.env.RUST_LOG = process.env.LOG_LEVEL || 'info';

// Spawn the actual codex-memory binary
const child = spawn('/Users/ladvien/.cargo/bin/codex-memory', ['mcp-stdio', '--skip-setup'], {
  stdio: 'inherit',
  env: process.env
});

// Forward exit codes
child.on('exit', (code) => {
  process.exit(code || 0);
});

// Handle errors
child.on('error', (err) => {
  console.error('Failed to start codex-memory:', err);
  process.exit(1);
});