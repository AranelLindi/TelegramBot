# Basis-Image mit Rust
FROM rust:latest

# Arbeitsverzeichnis setzen
WORKDIR /app

# Projektdateien kopieren 
COPY . .

# Abhängigkeiten und Binärdateien kompilieren
RUN cargo build --release

# Startkommando
CMD ["./target/release/TelegramBot"]
