# Galpha Proto - Build and Publish Commands

# Default recipe - show all commands
default:
    @just --list

# Build Rust code
build-rust:
    cargo build

# Build TypeScript code
build-ts:
    npm install
    npm run generate

# Build both Rust and TypeScript
build: build-rust build-ts

# Clean build artifacts
clean:
    cargo clean
    rm -rf dist node_modules

# Test Rust code
test-rust:
    cargo test

# Format Rust code
fmt:
    cargo fmt

# Check Rust code
check:
    cargo clippy

# Generate TypeScript SDK
generate-ts:
    @echo "📦 Generating TypeScript SDK..."
    npm install
    npm run generate
    @echo "✅ TypeScript SDK generated successfully in dist/"

# Test TypeScript package
test-ts:
    @echo "🧪 Testing TypeScript package..."
    npm run test
    @echo "✅ TypeScript package tests passed!"

# Test everything
test: test-rust test-ts

# Publish TypeScript SDK to npm (requires NPM_TOKEN)
publish-ts: generate-ts
    @echo "🚀 Publishing TypeScript SDK to npm..."
    @if [ -z "$NPM_TOKEN" ]; then \
        echo "❌ Error: NPM_TOKEN environment variable is not set"; \
        echo "Please set it with: export NPM_TOKEN=your_npm_token"; \
        exit 1; \
    fi
    @echo "//registry.npmjs.org/:_authToken=${NPM_TOKEN}" > .npmrc
    npm publish --access public
    @rm -f .npmrc
    @echo "✅ Published successfully!"

# Publish TypeScript SDK with version bump
publish-ts-patch: generate-ts
    @echo "🔼 Bumping patch version..."
    npm version patch
    @just publish-ts

publish-ts-minor: generate-ts
    @echo "🔼 Bumping minor version..."
    npm version minor
    @just publish-ts

publish-ts-major: generate-ts
    @echo "🔼 Bumping major version..."
    npm version major
    @just publish-ts

# Publish Rust crate to crates.io
publish-rust:
    @echo "🚀 Publishing Rust crate to crates.io..."
    cargo publish

# Quick development workflow
dev:
    @echo "🔄 Running development workflow..."
    @just fmt
    @just build
    @just test-rust
    @echo "✅ Development workflow completed!"

# Verify everything before publish
verify:
    @echo "🔍 Verifying project..."
    @just fmt
    @just check
    @just test-rust
    @just build
    @just test-ts
    @echo "✅ Verification completed!"
