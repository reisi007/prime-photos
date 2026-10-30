#!/usr/bin/env bash
# Signiert ein Linux-Release-Artefakt mit GPG (abgetrennte, ASCII-armored Signatur).
#
# Aufruf:
#   scripts/sign-linux-release.sh <artefakt> [<public-key-out>]
#
# Umgebung:
#   GPG_PRIVATE_KEY  (Pflicht) ASCII-armored privater Schlüssel
#   GPG_PASSPHRASE   (optional) Passphrase des Schlüssels; leer = kein Passphrase-Prompt
#   GPG_KEY_ID       (optional) Schlüssel-ID/Fingerprint, falls der Import mehrdeutig ist
#
# Ergebnis:
#   <artefakt>.asc                    abgetrennte ASCII-armored Signatur
#   <public-key-out>                  öffentlicher Schlüssel (nur mit zweitem Argument)
#
# Der Workflow ruft das Skript nur auf, wenn GPG_PRIVATE_KEY gesetzt ist; ein Fehler hier
# lässt den Job fehlschlagen, statt ein unsigniertes Artefakt still zu veröffentlichen.
set -euo pipefail

usage() {
  echo "usage: $0 <artifact> [<public-key-out>]" >&2
}

if [ "$#" -lt 1 ] || [ "$#" -gt 2 ]; then
  usage
  exit 2
fi

artifact="$1"
public_key_out="${2:-}"

if [ -z "${GPG_PRIVATE_KEY:-}" ]; then
  echo "error: GPG_PRIVATE_KEY is not set" >&2
  exit 1
fi

if [ ! -f "$artifact" ]; then
  echo "error: artifact not found: $artifact" >&2
  exit 1
fi

tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/prime-photos-gpg.XXXXXXXX")"
gnupg_home="$tmp_dir/gnupg"
key_file="$tmp_dir/private-key.asc"
mkdir -m 700 "$gnupg_home"

# Temporäres GNUPGHOME und Schlüsselmaterial immer aufräumen.
cleanup() {
  gpgconf --homedir "$gnupg_home" --kill all >/dev/null 2>&1 || true
  rm -rf "$tmp_dir"
}
trap cleanup EXIT

printf '%s\n' "$GPG_PRIVATE_KEY" >"$key_file"
chmod 600 "$key_file"

# Schlüssel importieren; die Passphrase wird hier nicht gebraucht.
gpg --batch --homedir "$gnupg_home" --import "$key_file" >/dev/null 2>&1

# Abgetrennte, ASCII-armored Signatur erzeugen.
sign_args=(--batch --yes --homedir "$gnupg_home" --pinentry-mode loopback)
if [ -n "${GPG_PASSPHRASE:-}" ]; then
  # Options must precede the positional file arguments.
  sign_args+=(--passphrase-fd 0)
fi
sign_args+=(--armor --detach-sign)
if [ -n "${GPG_KEY_ID:-}" ]; then
  sign_args+=(--local-user "$GPG_KEY_ID")
fi
sign_args+=(--output "$artifact.asc" "$artifact")

if [ -n "${GPG_PASSPHRASE:-}" ]; then
  printf '%s' "$GPG_PASSPHRASE" | gpg "${sign_args[@]}"
else
  gpg "${sign_args[@]}"
fi

# Signatur gegen das Artefakt prüfen.
gpg --batch --homedir "$gnupg_home" --verify "$artifact.asc" "$artifact"

# Öffentlichen Schlüssel für die Nutzerprüfung exportieren.
if [ -n "$public_key_out" ]; then
  if [ -n "${GPG_KEY_ID:-}" ]; then
    gpg --batch --homedir "$gnupg_home" --armor --export "$GPG_KEY_ID" >"$public_key_out"
  else
    gpg --batch --homedir "$gnupg_home" --armor --export >"$public_key_out"
  fi
  if [ ! -s "$public_key_out" ]; then
    echo "error: exported public key is empty: $public_key_out" >&2
    exit 1
  fi
fi

echo "signed: $artifact.asc"
if [ -n "$public_key_out" ]; then
  echo "public key: $public_key_out"
fi
