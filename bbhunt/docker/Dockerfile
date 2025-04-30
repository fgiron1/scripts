FROM python:3.10-slim

# Install system dependencies
RUN apt-get update && apt-get install -y \
    git \
    curl \
    wget \
    nmap \
    jq \
    && rm -rf /var/lib/apt/lists/*

# Install common security tools
RUN apt-get update && apt-get install -y \
    whois \
    dnsutils \
    nikto \
    dirb \
    sqlmap \
    && rm -rf /var/lib/apt/lists/*

# Create app directory and user
WORKDIR /app
RUN useradd -m bbhunt && \
    chown -R bbhunt:bbhunt /app

# Install Go (for additional tools)
RUN curl -sL https://golang.org/dl/go1.19.linux-amd64.tar.gz -o go.tar.gz && \
    tar -C /usr/local -xzf go.tar.gz && \
    rm go.tar.gz

# Add Go to PATH
ENV PATH=$PATH:/usr/local/go/bin:/root/go/bin

# Install Go tools
RUN go install -v github.com/projectdiscovery/subfinder/v2/cmd/subfinder@latest && \
    go install -v github.com/projectdiscovery/httpx/cmd/httpx@latest && \
    go install -v github.com/projectdiscovery/nuclei/v2/cmd/nuclei@latest && \
    go install -v github.com/ffuf/ffuf@latest && \
    go install -v github.com/tomnomnom/assetfinder@latest && \
    go install -v github.com/tomnomnom/waybackurls@latest

# Add Go bin to PATH
ENV PATH=$PATH:/root/go/bin

# Copy requirements first for better caching
COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt

# Copy application code
COPY . .

# Create necessary directories
RUN mkdir -p /app/data /app/config

# Download wordlists
RUN mkdir -p /app/wordlists && \
    cd /app/wordlists && \
    git clone https://github.com/danielmiessler/SecLists.git && \
    git clone https://github.com/assetnote/commonspeak2-wordlists.git

# Configure Nuclei templates
RUN nuclei -ut

# Default entrypoint
ENTRYPOINT ["python", "-m", "bbhunt.bbhunt"]
