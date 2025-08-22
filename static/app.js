// Memory Harvester Configuration UI
class HarvesterUI {
    constructor() {
        this.config = {};
        this.statistics = {};
        this.isLoading = false;
        this.init();
    }

    async init() {
        this.setupEventListeners();
        this.setupTabs();
        await this.loadInitialData();
    }

    setupEventListeners() {
        // Configuration controls
        document.getElementById('harvester-enabled').addEventListener('change', (e) => {
            this.updateConfigValue('enabled', e.target.checked);
        });

        document.getElementById('privacy-mode').addEventListener('change', (e) => {
            this.updateConfigValue('privacy_mode', e.target.checked);
            this.togglePrivacyMode(e.target.checked);
        });

        document.getElementById('confidence-threshold').addEventListener('input', (e) => {
            const value = parseFloat(e.target.value);
            document.getElementById('confidence-value').textContent = value.toFixed(2);
            this.updateConfigValue('confidence_threshold', value);
        });

        document.getElementById('message-count').addEventListener('input', (e) => {
            this.updateConfigValue('message_trigger_count', parseInt(e.target.value));
        });

        document.getElementById('time-interval').addEventListener('input', (e) => {
            this.updateConfigValue('time_trigger_minutes', parseInt(e.target.value));
        });

        // Action buttons
        document.getElementById('save-config').addEventListener('click', () => {
            this.saveConfiguration();
        });

        document.getElementById('reset-config').addEventListener('click', () => {
            this.resetConfiguration();
        });

        // Statistics
        document.getElementById('refresh-memories').addEventListener('click', () => {
            this.loadRecentMemories();
        });

        // Memory filtering
        document.getElementById('pattern-filter').addEventListener('change', () => {
            this.loadRecentMemories();
        });

        document.getElementById('confidence-filter').addEventListener('input', (e) => {
            const value = parseFloat(e.target.value);
            document.getElementById('confidence-filter-value').textContent = value.toFixed(1);
            this.loadRecentMemories();
        });

        // Export functionality
        document.getElementById('download-export').addEventListener('click', () => {
            this.downloadExport();
        });

        document.getElementById('preview-export').addEventListener('click', () => {
            this.previewExport();
        });

        // Toast close
        document.getElementById('toast-close').addEventListener('click', () => {
            this.hideToast();
        });
    }

    setupTabs() {
        const tabButtons = document.querySelectorAll('.tab-button');
        const tabContents = document.querySelectorAll('.tab-content');

        tabButtons.forEach(button => {
            button.addEventListener('click', () => {
                const targetTab = button.dataset.tab;

                // Update active states
                tabButtons.forEach(btn => btn.classList.remove('active'));
                tabContents.forEach(content => content.classList.remove('active'));

                button.classList.add('active');
                document.getElementById(targetTab).classList.add('active');

                // Load tab-specific data
                this.onTabChange(targetTab);
            });
        });
    }

    async onTabChange(tab) {
        switch (tab) {
            case 'configuration':
                await this.loadConfiguration();
                break;
            case 'statistics':
                await this.loadStatistics();
                break;
            case 'recent-memories':
                await this.loadRecentMemories();
                break;
            case 'export':
                await this.loadExportPreview();
                break;
        }
    }

    async loadInitialData() {
        try {
            await this.checkStatus();
            await this.loadConfiguration();
        } catch (error) {
            this.showToast('Failed to load initial data: ' + error.message, 'error');
        }
    }

    async checkStatus() {
        try {
            const response = await fetch('/api/harvester/status');
            const status = await response.json();
            
            this.updateStatusIndicator(status);
        } catch (error) {
            this.updateStatusIndicator({ active: false, health_status: 'error' });
        }
    }

    updateStatusIndicator(status) {
        const indicator = document.getElementById('status-indicator');
        const statusText = document.getElementById('status-text');
        
        if (status.active) {
            statusText.textContent = `Active • ${status.messages_processed || 0} messages processed`;
            indicator.className = 'status-indicator active';
        } else {
            statusText.textContent = 'Inactive';
            indicator.className = 'status-indicator inactive';
        }
    }

    async loadConfiguration() {
        try {
            const response = await fetch('/api/config/harvester');
            this.config = await response.json();
            
            this.populateConfigurationForm();
            this.setupPatternSelection();
        } catch (error) {
            this.showToast('Failed to load configuration: ' + error.message, 'error');
        }
    }

    populateConfigurationForm() {
        const config = this.config;
        
        document.getElementById('harvester-enabled').checked = config.enabled || false;
        document.getElementById('privacy-mode').checked = config.privacy_mode || false;
        
        document.getElementById('confidence-threshold').value = config.confidence_threshold || 0.7;
        document.getElementById('confidence-value').textContent = (config.confidence_threshold || 0.7).toFixed(2);
        
        document.getElementById('message-count').value = config.message_trigger_count || 10;
        document.getElementById('time-interval').value = config.time_trigger_minutes || 5;
        
        this.togglePrivacyMode(config.privacy_mode || false);
    }

    setupPatternSelection() {
        const patternGrid = document.getElementById('pattern-grid');
        patternGrid.innerHTML = '';

        if (!this.config.pattern_types) return;

        this.config.pattern_types.forEach(patternType => {
            const patternItem = document.createElement('div');
            patternItem.className = `pattern-item ${patternType.enabled ? 'enabled' : ''}`;
            
            patternItem.innerHTML = `
                <div class="pattern-header">
                    <div class="pattern-title">${this.formatPatternType(patternType.pattern_type)}</div>
                    <label class="toggle-switch">
                        <input type="checkbox" ${patternType.enabled ? 'checked' : ''} 
                               data-pattern="${patternType.pattern_type}">
                        <span class="slider"></span>
                    </label>
                </div>
                <div class="pattern-description">${patternType.description}</div>
                <div class="pattern-count">${patternType.patterns.length} patterns defined</div>
            `;

            // Add event listener for pattern toggle
            const checkbox = patternItem.querySelector('input[type="checkbox"]');
            checkbox.addEventListener('change', (e) => {
                this.togglePatternType(patternType.pattern_type, e.target.checked);
                patternItem.classList.toggle('enabled', e.target.checked);
            });

            patternGrid.appendChild(patternItem);
        });
    }

    async loadStatistics() {
        try {
            const response = await fetch('/api/harvester/stats');
            this.statistics = await response.json();
            
            this.populateStatistics();
        } catch (error) {
            this.showToast('Failed to load statistics: ' + error.message, 'error');
        }
    }

    populateStatistics() {
        const stats = this.statistics;
        
        document.getElementById('messages-processed').textContent = 
            this.formatNumber(stats.total_messages_processed || 0);
        document.getElementById('patterns-extracted').textContent = 
            this.formatNumber(stats.total_patterns_extracted || 0);
        document.getElementById('memories-stored').textContent = 
            this.formatNumber(stats.total_memories_stored || 0);
        document.getElementById('duplicates-filtered').textContent = 
            this.formatNumber(stats.total_duplicates_filtered || 0);

        // Update confidence distribution
        if (stats.confidence_score_distribution) {
            const total = stats.confidence_score_distribution.high_confidence + 
                         stats.confidence_score_distribution.medium_confidence + 
                         stats.confidence_score_distribution.low_confidence;

            this.updateConfidenceBar('high', stats.confidence_score_distribution.high_confidence, total);
            this.updateConfidenceBar('medium', stats.confidence_score_distribution.medium_confidence, total);
            this.updateConfidenceBar('low', stats.confidence_score_distribution.low_confidence, total);
        }
    }

    updateConfidenceBar(level, value, total) {
        const bar = document.getElementById(`${level}-confidence-bar`);
        const valueSpan = document.getElementById(`${level}-confidence-value`);
        
        const percentage = total > 0 ? (value / total) * 100 : 0;
        bar.style.width = `${percentage}%`;
        valueSpan.textContent = value.toString();
    }

    async loadRecentMemories() {
        try {
            const patternFilter = document.getElementById('pattern-filter').value;
            const confidenceFilter = parseFloat(document.getElementById('confidence-filter').value);
            
            const params = new URLSearchParams({
                limit: '20',
                ...(patternFilter && { pattern_type: patternFilter }),
                ...(confidenceFilter > 0 && { min_confidence: confidenceFilter.toString() })
            });

            const response = await fetch(`/api/harvester/recent?${params}`);
            const memories = await response.json();
            
            this.populateMemoriesList(memories);
        } catch (error) {
            this.showToast('Failed to load recent memories: ' + error.message, 'error');
        }
    }

    populateMemoriesList(memories) {
        const memoriesList = document.getElementById('memories-list');
        
        if (!memories || memories.length === 0) {
            memoriesList.innerHTML = `
                <div style="text-align: center; padding: 2rem; color: #64748b;">
                    No memories found matching your criteria.
                </div>
            `;
            return;
        }

        memoriesList.innerHTML = memories.map(memory => `
            <div class="memory-item">
                <div class="memory-header">
                    <div class="memory-type">${this.formatPatternType(memory.pattern_type)}</div>
                    <div class="memory-confidence">${(memory.confidence * 100).toFixed(0)}%</div>
                </div>
                <div class="memory-content">${memory.content}</div>
                <div class="memory-meta">
                    <span>ID: ${memory.id.substring(0, 8)}...</span>
                    <span>Tier: ${memory.tier}</span>
                    <span>Importance: ${memory.importance_score.toFixed(2)}</span>
                    <span>${this.formatRelativeTime(memory.created_at)}</span>
                </div>
            </div>
        `).join('');
    }

    async loadExportPreview() {
        try {
            const response = await fetch('/api/harvester/export');
            const exportData = await response.json();
            
            this.populateExportStats(exportData);
        } catch (error) {
            this.showToast('Failed to load export preview: ' + error.message, 'error');
        }
    }

    populateExportStats(exportData) {
        const exportStats = document.getElementById('export-stats');
        
        exportStats.innerHTML = `
            <h3>Export Preview</h3>
            <p><strong>Total Memories:</strong> ${exportData.total_memories}</p>
            <p><strong>Export Size:</strong> ~${this.estimateFileSize(exportData)}</p>
            <p><strong>Generated:</strong> ${this.formatDateTime(exportData.export_timestamp)}</p>
            
            <h4>Pattern Type Breakdown:</h4>
            ${Object.entries(exportData.statistics.by_pattern_type)
                .map(([type, count]) => `<p>${this.formatPatternType(type)}: ${count}</p>`)
                .join('')}
        `;
    }

    async saveConfiguration() {
        if (this.isLoading) return;
        
        this.isLoading = true;
        const saveButton = document.getElementById('save-config');
        const originalText = saveButton.textContent;
        saveButton.textContent = 'Saving...';
        saveButton.disabled = true;

        try {
            const response = await fetch('/api/config/harvester', {
                method: 'PUT',
                headers: {
                    'Content-Type': 'application/json'
                },
                body: JSON.stringify(this.getFormData())
            });

            if (response.ok) {
                this.showToast('Configuration saved successfully!', 'success');
            } else {
                throw new Error('Failed to save configuration');
            }
        } catch (error) {
            this.showToast('Failed to save configuration: ' + error.message, 'error');
        } finally {
            this.isLoading = false;
            saveButton.textContent = originalText;
            saveButton.disabled = false;
        }
    }

    resetConfiguration() {
        if (confirm('Are you sure you want to reset all configuration to defaults? This cannot be undone.')) {
            // Reset to default values
            document.getElementById('harvester-enabled').checked = true;
            document.getElementById('privacy-mode').checked = false;
            document.getElementById('confidence-threshold').value = 0.7;
            document.getElementById('confidence-value').textContent = '0.70';
            document.getElementById('message-count').value = 10;
            document.getElementById('time-interval').value = 5;
            
            this.togglePrivacyMode(false);
            this.showToast('Configuration reset to defaults', 'success');
        }
    }

    async downloadExport() {
        try {
            const response = await fetch('/api/harvester/export');
            const exportData = await response.json();
            
            const format = document.querySelector('input[name="export-format"]:checked').value;
            const includeMetadata = document.getElementById('include-metadata').checked;
            const anonymize = document.getElementById('anonymize-data').checked;
            
            let content, filename, mimeType;
            
            if (format === 'json') {
                content = JSON.stringify(exportData, null, 2);
                filename = 'memory-export.json';
                mimeType = 'application/json';
            } else {
                content = this.convertToCSV(exportData, includeMetadata);
                filename = 'memory-export.csv';
                mimeType = 'text/csv';
            }
            
            this.downloadFile(content, filename, mimeType);
            this.showToast('Export downloaded successfully!', 'success');
        } catch (error) {
            this.showToast('Failed to download export: ' + error.message, 'error');
        }
    }

    async previewExport() {
        try {
            const response = await fetch('/api/harvester/export');
            const exportData = await response.json();
            
            const format = document.querySelector('input[name="export-format"]:checked').value;
            let preview;
            
            if (format === 'json') {
                preview = JSON.stringify(exportData, null, 2).substring(0, 1000) + '...';
            } else {
                preview = this.convertToCSV(exportData, true).substring(0, 1000) + '...';
            }
            
            const previewWindow = window.open('', '_blank');
            previewWindow.document.write(`
                <html>
                    <head><title>Export Preview</title></head>
                    <body>
                        <h2>Export Preview (${format.toUpperCase()})</h2>
                        <pre style="white-space: pre-wrap; font-family: monospace;">${preview}</pre>
                    </body>
                </html>
            `);
        } catch (error) {
            this.showToast('Failed to generate preview: ' + error.message, 'error');
        }
    }

    // Utility methods
    updateConfigValue(key, value) {
        if (!this.config) this.config = {};
        this.config[key] = value;
    }

    togglePatternType(patternType, enabled) {
        // Update pattern type configuration
        this.showToast(`${this.formatPatternType(patternType)} ${enabled ? 'enabled' : 'disabled'}`, 'success');
    }

    togglePrivacyMode(enabled) {
        const configSections = document.querySelectorAll('.config-section');
        configSections.forEach(section => {
            if (section.querySelector('h2').textContent !== 'General Settings') {
                section.style.opacity = enabled ? '0.5' : '1';
                section.style.pointerEvents = enabled ? 'none' : 'auto';
            }
        });
    }

    getFormData() {
        return {
            enabled: document.getElementById('harvester-enabled').checked,
            privacy_mode: document.getElementById('privacy-mode').checked,
            confidence_threshold: parseFloat(document.getElementById('confidence-threshold').value),
            message_trigger_count: parseInt(document.getElementById('message-count').value),
            time_trigger_minutes: parseInt(document.getElementById('time-interval').value),
        };
    }

    convertToCSV(exportData, includeMetadata) {
        const headers = ['ID', 'Content', 'Pattern Type', 'Confidence', 'Created At', 'Tier', 'Importance'];
        if (includeMetadata) {
            headers.push('Tags', 'Metadata');
        }

        const rows = exportData.memories.map(memory => {
            const row = [
                memory.id,
                `"${memory.content.replace(/"/g, '""')}"`,
                memory.pattern_type,
                memory.confidence,
                memory.created_at,
                memory.tier,
                memory.importance_score
            ];
            
            if (includeMetadata) {
                row.push(memory.tags.join(';'), JSON.stringify(memory.metadata));
            }
            
            return row.join(',');
        });

        return [headers.join(','), ...rows].join('\n');
    }

    downloadFile(content, filename, mimeType) {
        const blob = new Blob([content], { type: mimeType });
        const url = URL.createObjectURL(blob);
        
        const link = document.createElement('a');
        link.href = url;
        link.download = filename;
        document.body.appendChild(link);
        link.click();
        document.body.removeChild(link);
        
        URL.revokeObjectURL(url);
    }

    showToast(message, type = 'success') {
        const toast = document.getElementById('toast');
        const messageSpan = document.getElementById('toast-message');
        
        messageSpan.textContent = message;
        toast.className = `toast ${type} show`;
        
        setTimeout(() => {
            toast.classList.remove('show');
        }, 5000);
    }

    hideToast() {
        document.getElementById('toast').classList.remove('show');
    }

    formatPatternType(type) {
        return type.charAt(0).toUpperCase() + type.slice(1).toLowerCase();
    }

    formatNumber(num) {
        return new Intl.NumberFormat().format(num);
    }

    formatRelativeTime(dateStr) {
        const date = new Date(dateStr);
        const now = new Date();
        const diffMs = now - date;
        
        const diffMinutes = Math.floor(diffMs / (1000 * 60));
        const diffHours = Math.floor(diffMs / (1000 * 60 * 60));
        const diffDays = Math.floor(diffMs / (1000 * 60 * 60 * 24));
        
        if (diffMinutes < 1) return 'Just now';
        if (diffMinutes < 60) return `${diffMinutes}m ago`;
        if (diffHours < 24) return `${diffHours}h ago`;
        if (diffDays < 7) return `${diffDays}d ago`;
        
        return date.toLocaleDateString();
    }

    formatDateTime(dateStr) {
        return new Date(dateStr).toLocaleString();
    }

    estimateFileSize(exportData) {
        const sizeBytes = JSON.stringify(exportData).length;
        if (sizeBytes < 1024) return `${sizeBytes} bytes`;
        if (sizeBytes < 1024 * 1024) return `${(sizeBytes / 1024).toFixed(1)} KB`;
        return `${(sizeBytes / (1024 * 1024)).toFixed(1)} MB`;
    }
}

// Initialize the UI when DOM is loaded
document.addEventListener('DOMContentLoaded', () => {
    new HarvesterUI();
});