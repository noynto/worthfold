# syntax=docker/dockerfile:1

# worthfold : un binaire statique (musl) dans une image qui ne contient rien d'autre que lui et les certificats.
#   docker build -t worthfold .
#
# Étapes :
#   1. builder : compile en release (LTO, un seul codegen-unit, symboles retirés), Tailwind compris.
#   2. final   : distroless « static » (certificats, utilisateur non-root, pas de shell ni de gestionnaire de paquets).

ARG RUST_VERSION=1.99
ARG TAILWIND_VERSION=v4.3.5

# ── 1. Compilation ───────────────────────────────────────────────────────────
FROM rust:${RUST_VERSION}-alpine AS builder
ARG TAILWIND_VERSION

# build-base : compilateur C pour les dépendances qui en ont besoin (aws-lc, via rustls) ; cmake : idem.
RUN apk add --no-cache build-base cmake curl

# Tailwind CSS (exécutable autonome, version figée et somme de contrôle vérifiée) : build.rs compile la feuille de style.
RUN set -eux; \
    case "$(uname -m)" in x86_64) arch=x64 ;; aarch64) arch=arm64 ;; *) echo "architecture non prise en charge" >&2; exit 1 ;; esac; \
    base="https://github.com/tailwindlabs/tailwindcss/releases/download/${TAILWIND_VERSION}"; \
    cd /tmp; \
    curl -sSfLO "${base}/tailwindcss-linux-${arch}-musl"; \
    curl -sSfLO "${base}/sha256sums.txt"; \
    grep " \(\./\)\?tailwindcss-linux-${arch}-musl\$" sha256sums.txt | sed 's#\./##' | sha256sum -c -; \
    install -m 755 "tailwindcss-linux-${arch}-musl" /usr/local/bin/tailwindcss; \
    rm -f /tmp/tailwindcss-* /tmp/sha256sums.txt
ENV TAILWINDCSS=/usr/local/bin/tailwindcss

WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

# Les caches (registre, dépôts git, dossier target) vivent dans BuildKit, pas dans l'image : une recompilation après
# un changement de code ne repart pas de zéro. Le binaire est copié hors du cache dans la même commande.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked -p worthfold-bootstrap \
    && install -m 755 target/release/worthfold /worthfold

# ── 2. Image finale ──────────────────────────────────────────────────────────
FROM gcr.io/distroless/static-debian12:nonroot

LABEL org.opencontainers.image.title="worthfold" \
      org.opencontainers.image.description="Suivi de patrimoine personnel" \
      org.opencontainers.image.licenses="MIT"

COPY --from=builder /worthfold /worthfold

# Dans un conteneur, il faut écouter sur toutes les interfaces pour que le port publié soit joignable ;
# c'est le compose qui ne le publie que sur 127.0.0.1 (voir compose.yaml).
ENV WORTHFOLD_BIND_ADDRESS=0.0.0.0 \
    WORTHFOLD_PORT=3000
EXPOSE 3000

# Aucun shell dans l'image : la sonde est le binaire lui-même.
HEALTHCHECK --interval=30s --timeout=5s --start-period=30s --retries=3 \
    CMD ["/worthfold", "healthcheck"]

USER nonroot
ENTRYPOINT ["/worthfold"]
