# Build stage
FROM --platform=$BUILDPLATFORM rust:1.94 AS builder

ARG TARGETPLATFORM
ARG BUILDPLATFORM

WORKDIR /app

# Install cross-compilation tools
RUN case "$TARGETPLATFORM" in \
    "linux/amd64") echo "x86_64-unknown-linux-musl" > /tmp/rust-target ;; \
    "linux/arm64") echo "aarch64-unknown-linux-musl" > /tmp/rust-target ;; \
    *) echo "Unsupported platform: $TARGETPLATFORM" && exit 1 ;; \
    esac && \
    rustup target add $(cat /tmp/rust-target)

# Install cross-compilation dependencies
RUN apt-get update && \
    apt-get install -y musl-tools gcc-aarch64-linux-gnu wget ca-certificates && \
    rm -rf /var/lib/apt/lists/*

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Copy source code
COPY src ./src

# Configure cargo for cross-compilation
RUN mkdir -p .cargo && \
    echo '[target.aarch64-unknown-linux-musl]' > .cargo/config.toml && \
    echo 'linker = "aarch64-linux-gnu-gcc"' >> .cargo/config.toml

# Download fortune files in the builder stage
RUN mkdir -p /app/fortunes && \
    cd /app/fortunes && \
    wget -q https://raw.githubusercontent.com/bmc/fortunes/master/fortunes && \
    wget -q https://raw.githubusercontent.com/bmc/fortunes/master/art && \
    wget -q https://raw.githubusercontent.com/bmc/fortunes/master/computers && \
    wget -q https://raw.githubusercontent.com/bmc/fortunes/master/cookie && \
    wget -q https://raw.githubusercontent.com/bmc/fortunes/master/definitions && \
    wget -q https://raw.githubusercontent.com/bmc/fortunes/master/drugs && \
    wget -q https://raw.githubusercontent.com/bmc/fortunes/master/education && \
    wget -q https://raw.githubusercontent.com/bmc/fortunes/master/ethnic && \
    wget -q https://raw.githubusercontent.com/bmc/fortunes/master/food && \
    wget -q https://raw.githubusercontent.com/bmc/fortunes/master/fortunes2 && \
    wget -q https://raw.githubusercontent.com/bmc/fortunes/master/goedel && \
    wget -q https://raw.githubusercontent.com/bmc/fortunes/master/humorists && \
    wget -q https://raw.githubusercontent.com/bmc/fortunes/master/kids && \
    wget -q https://raw.githubusercontent.com/bmc/fortunes/master/law && \
    wget -q https://raw.githubusercontent.com/bmc/fortunes/master/limerick && \
    wget -q https://raw.githubusercontent.com/bmc/fortunes/master/linux && \
    wget -q https://raw.githubusercontent.com/bmc/fortunes/master/literature && \
    wget -q https://raw.githubusercontent.com/bmc/fortunes/master/love && \
    wget -q https://raw.githubusercontent.com/bmc/fortunes/master/magic && \
    wget -q https://raw.githubusercontent.com/bmc/fortunes/master/medicine && \
    wget -q https://raw.githubusercontent.com/bmc/fortunes/master/men-women && \
    wget -q https://raw.githubusercontent.com/bmc/fortunes/master/miscellaneous && \
    wget -q https://raw.githubusercontent.com/bmc/fortunes/master/news && \
    wget -q https://raw.githubusercontent.com/bmc/fortunes/master/people && \
    wget -q https://raw.githubusercontent.com/bmc/fortunes/master/pets && \
    wget -q https://raw.githubusercontent.com/bmc/fortunes/master/platitudes && \
    wget -q https://raw.githubusercontent.com/bmc/fortunes/master/politics && \
    wget -q https://raw.githubusercontent.com/bmc/fortunes/master/privacy && \
    wget -q https://raw.githubusercontent.com/bmc/fortunes/master/riddles && \
    wget -q https://raw.githubusercontent.com/bmc/fortunes/master/science && \
    wget -q https://raw.githubusercontent.com/bmc/fortunes/master/songs-poems && \
    wget -q https://raw.githubusercontent.com/bmc/fortunes/master/sports && \
    wget -q https://raw.githubusercontent.com/bmc/fortunes/master/startrek && \
    wget -q https://raw.githubusercontent.com/bmc/fortunes/master/wisdom && \
    wget -q https://raw.githubusercontent.com/bmc/fortunes/master/work && \
    wget -q https://raw.githubusercontent.com/bmc/fortunes/master/zippy || true

# Build release binary for target platform
RUN RUST_TARGET=$(cat /tmp/rust-target) && \
    cargo build --release --target $RUST_TARGET && \
    cp target/$RUST_TARGET/release/fortune-mcp-server target/release/fortune-mcp-server

# Runtime stage
FROM scratch

WORKDIR /app

# Copy the statically linked binary from builder
COPY --from=builder /app/target/release/fortune-mcp-server /app/fortune-server

# Copy fortune files from builder
COPY --from=builder /app/fortunes /app/fortunes

ENTRYPOINT ["/app/fortune-server"]
