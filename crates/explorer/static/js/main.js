// Revive Compiler Explorer - Interactive YUL ↔ RISC-V Navigation
class ReviveExplorer {
    constructor() {
        this.mapping = null;
        this.yulData = null;
        this.assemblyData = null;
        this.currentHighlight = null;
        
        this.yulElement = document.getElementById('yul-source');
        this.assemblyElement = document.getElementById('assembly-output');
        this.yulInfo = document.getElementById('yul-info');
        this.asmInfo = document.getElementById('asm-info');
        
        this.init();
    }
    
    async init() {
        console.log('🚀 Initializing Revive Compiler Explorer...');
        
        try {
            await this.loadData();
            this.setupEventListeners();
            this.renderContent();
            console.log('✅ Initialization complete');
        } catch (error) {
            console.error('❌ Failed to initialize:', error);
            this.showError('Failed to load analysis data: ' + error.message);
        }
    }
    
    async loadData() {
        console.log('📡 Loading analysis data...');
        
        // Load all data in parallel for better performance
        const [yulResponse, assemblyResponse, mappingResponse] = await Promise.all([
            fetch('/api/yul').then(r => r.json()),
            fetch('/api/assembly').then(r => r.json()),
            fetch('/api/mapping').then(r => r.json())
        ]);
        
        this.yulData = yulResponse;
        this.assemblyData = assemblyResponse;
        this.mapping = mappingResponse;
        
        console.log(`📊 Loaded: ${this.yulData.line_count} YUL lines, ${this.assemblyData.instruction_count} instructions, ${this.mapping.mapping_count} mappings`);
    }
    
    renderContent() {
        this.renderYulSource();
        this.renderAssembly();
        this.updateInfo();
    }
    
    renderYulSource() {
        if (!this.yulData) return;
        
        const lines = this.yulData.content.split('\n');
        const html = lines.map((line, index) => 
            `<span class="code-line" data-line="${index + 1}">${this.escapeHtml(line)}</span>`
        ).join('\n');
        
        this.yulElement.innerHTML = html;
    }
    
    renderAssembly() {
        if (!this.assemblyData) return;
        
        const html = this.assemblyData.instructions.map(instr => 
            `<div class="assembly-instruction" data-address="${instr.address}">` +
            `<span class="assembly-address">${instr.address}</span>` +
            `<span class="assembly-bytes">${instr.bytes}</span>` +
            `<span class="assembly-instruction-text">${this.escapeHtml(instr.instruction)}</span>` +
            `</div>`
        ).join('\n');
        
        this.assemblyElement.innerHTML = html;
    }
    
    updateInfo() {
        if (this.yulData) {
            this.yulInfo.textContent = `${this.yulData.line_count} lines • ${this.yulData.filename}`;
        }
        
        if (this.assemblyData) {
            this.asmInfo.textContent = `${this.assemblyData.instruction_count} instructions`;
        }
    }
    
    setupEventListeners() {
        // Make YUL lines clickable for navigation
        this.yulElement.addEventListener('click', (event) => {
            const lineElement = event.target.closest('.code-line');
            if (lineElement) {
                const lineNumber = parseInt(lineElement.dataset.line);
                this.navigateToLine(lineNumber);
            }
        });
        
        // Make assembly instructions clickable for reverse navigation
        this.assemblyElement.addEventListener('click', (event) => {
            const instrElement = event.target.closest('.assembly-instruction');
            if (instrElement) {
                const address = instrElement.dataset.address;
                this.navigateToAddress(address);
            }
        });
    }
    
    async navigateToLine(lineNumber) {
        console.log(`🎯 Navigating to YUL line ${lineNumber}`);
        
        try {
            // Get assembly addresses for this line
            const response = await fetch(`/api/assembly-for-line?line=${lineNumber}`);
            const addresses = await response.json();
            
            this.highlightYulLine(lineNumber);
            this.highlightAssemblyAddresses(addresses);
            
            // Scroll to first assembly instruction if any
            if (addresses.length > 0) {
                this.scrollToAssemblyAddress(addresses[0]);
            }
            
        } catch (error) {
            console.error('Failed to navigate to line:', error);
        }
    }
    
    navigateToAddress(address) {
        console.log(`🎯 Reverse navigation from address ${address}`);
        
        // Find YUL line for this address from mapping
        const mapping = this.mapping.mappings.find(m => 
            m.assembly_addresses.includes(address)
        );
        
        if (mapping) {
            this.highlightYulLine(mapping.yul_line);
            this.highlightAssemblyAddresses([address]);
            this.scrollToYulLine(mapping.yul_line);
        }
    }
    
    highlightYulLine(lineNumber) {
        // Clear previous highlights
        this.yulElement.querySelectorAll('.highlighted-line, .selected-line')
            .forEach(el => el.classList.remove('highlighted-line', 'selected-line'));
        
        // Highlight the selected line
        const lineElement = this.yulElement.querySelector(`[data-line="${lineNumber}"]`);
        if (lineElement) {
            lineElement.classList.add('selected-line');
        }
    }
    
    highlightAssemblyAddresses(addresses) {
        // Clear previous assembly highlights
        this.assemblyElement.querySelectorAll('.highlighted-line, .selected-line')
            .forEach(el => el.classList.remove('highlighted-line', 'selected-line'));
        
        // Highlight corresponding assembly instructions
        addresses.forEach(address => {
            const instrElement = this.assemblyElement.querySelector(`[data-address="${address}"]`);
            if (instrElement) {
                instrElement.classList.add('highlighted-line');
            }
        });
    }
    
    scrollToYulLine(lineNumber) {
        const lineElement = this.yulElement.querySelector(`[data-line="${lineNumber}"]`);
        if (lineElement) {
            lineElement.scrollIntoView({ behavior: 'smooth', block: 'center' });
        }
    }
    
    scrollToAssemblyAddress(address) {
        const instrElement = this.assemblyElement.querySelector(`[data-address="${address}"]`);
        if (instrElement) {
            instrElement.scrollIntoView({ behavior: 'smooth', block: 'center' });
        }
    }
    
    escapeHtml(text) {
        const div = document.createElement('div');
        div.textContent = text;
        return div.innerHTML;
    }
    
    showError(message) {
        this.yulElement.innerHTML = `<div style="color: red; padding: 20px;">Error: ${message}</div>`;
        this.assemblyElement.innerHTML = `<div style="color: red; padding: 20px;">Error: ${message}</div>`;
    }
}

// Initialize when DOM loads
document.addEventListener('DOMContentLoaded', () => {
    console.log('🔍 Starting Revive Compiler Explorer...');
    new ReviveExplorer();
});
