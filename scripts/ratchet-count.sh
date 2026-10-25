#!/usr/bin/env bash
set -euo pipefail

# allow-attributes skips comments and string literals, then counts every
# allow(...)/expect(...) occurrence inside an attribute group, including nested
# cfg_attr suppressions and inner attributes.
SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd "$SCRIPT_DIR/.." && pwd)

usage_error() {
  printf 'ratchet-count.sh: %s\n' "$1" >&2
  printf 'Aufruf: ratchet-count.sh oversized-rust-files [MAX] | allow-attributes\n' >&2
  exit 2
}

case "${1-}" in
  oversized-rust-files)
    if [ "$#" -gt 2 ]; then
      usage_error 'zu viele Argumente für oversized-rust-files'
    fi
    max=${2:-500}
    case "$max" in
      ''|*[!0-9]*) usage_error 'MAX muss eine nichtnegative Ganzzahl sein' ;;
    esac

    tmp=$(mktemp "${TMPDIR:-/tmp}/ratchet-count.XXXXXX") || {
      printf 'ratchet-count.sh: temporäre Datei für den Rust-Datei-Scan konnte nicht erstellt werden\n' >&2
      exit 2
    }
    trap 'rm -f "$tmp"' EXIT
    if ! find "$REPO_ROOT" \
      \( -type d \( -path "$REPO_ROOT/target" -o -path "$REPO_ROOT/.git" \) -prune \) -o \
      \( -type f -name '*.rs' -print0 \) >"$tmp"; then
      printf 'ratchet-count.sh: Rust-Datei-Scan fehlgeschlagen\n' >&2
      exit 2
    fi

    count=0
    while IFS= read -r -d '' file; do
      lines=$(awk 'END { print NR }' "$file")
      if [ "$lines" -gt "$max" ]; then
        count=$((count + 1))
      fi
    done < "$tmp"
    printf '%s\n' "$count"
    ;;

  allow-attributes)
    if [ "$#" -ne 1 ]; then
      usage_error 'allow-attributes nimmt keine Argumente an'
    fi

    if [ ! -d "$REPO_ROOT/src" ]; then
      printf 'ratchet-count.sh: Quellverzeichnis fehlt: %s/src\n' "$REPO_ROOT" >&2
      exit 2
    fi

    tmp=$(mktemp "${TMPDIR:-/tmp}/ratchet-count.XXXXXX") || {
      printf 'ratchet-count.sh: temporäre Datei für den Attribut-Scan konnte nicht erstellt werden\n' >&2
      exit 2
    }
    trap 'rm -f "$tmp"' EXIT
    if ! find "$REPO_ROOT/src" -type f -name '*.rs' -print0 >"$tmp"; then
      printf 'ratchet-count.sh: Attribut-Scan fehlgeschlagen\n' >&2
      exit 2
    fi

    count=0
    while IFS= read -r -d '' file; do
      if ! matches=$(awk '
        function char_literal_end(line, start, j, char) {
          j = start + 1
          char = substr(line, j, 1)
          if (char == "\\") {
            j++
            char = substr(line, j, 1)
            if (char == "u" && substr(line, j + 1, 1) == "{") {
              j += 2
              while (j <= length(line) && substr(line, j, 1) != "}") {
                j++
              }
              if (substr(line, j, 1) != "}") {
                return 0
              }
              j++
            } else if (char == "x") {
              j += 3
            } else if (char != "") {
              j++
            }
          } else if (char != "") {
            j++
          }
          return substr(line, j, 1) == "\047" ? j : 0
        }
        {
          line_code = ""
          for (i = 1; i <= length($0);) {
            char = substr($0, i, 1)
            pair = substr($0, i, 2)

            if (block_comment_depth > 0) {
              if (pair == "/*") {
                block_comment_depth++
                line_code = line_code "  "
                i += 2
              } else if (pair == "*/") {
                block_comment_depth--
                line_code = line_code "  "
                i += 2
              } else {
                line_code = line_code " "
                i++
              }
            } else if (string_mode == "quoted") {
              line_code = line_code " "
              if (string_escape) {
                string_escape = 0
              } else if (char == "\\") {
                string_escape = 1
              } else if (char == "\"") {
                string_mode = ""
              }
              i++
            } else if (string_mode == "raw") {
              if (char == "\"" && substr($0, i + 1, raw_hash_count) == raw_hashes) {
                line_code = line_code " "
                i += 1 + raw_hash_count
                string_mode = ""
                raw_hash_count = 0
                raw_hashes = ""
              } else {
                line_code = line_code " "
                i++
              }
            } else if (pair == "//") {
              line_code = line_code " "
              break
            } else if (pair == "/*") {
              block_comment_depth = 1
              line_code = line_code "  "
              i += 2
            } else if (char == "r") {
              j = i + 1
              hashes = ""
              while (substr($0, j, 1) == "#") {
                hashes = hashes "#"
                j++
              }
              if (substr($0, j, 1) == "\"") {
                string_mode = "raw"
                raw_hash_count = length(hashes)
                raw_hashes = hashes
                line_code = line_code " "
                i = j + 1
              } else {
                line_code = line_code char
                i++
              }
            } else if (char == "\047") {
              char_end = char_literal_end($0, i)
              if (char_end > i) {
                line_code = line_code " "
                i = char_end + 1
              } else {
                line_code = line_code char
                i++
              }
            } else if (char == "\"") {
              string_mode = "quoted"
              string_escape = 0
              line_code = line_code " "
              i++
            } else {
              line_code = line_code char
              i++
            }
          }

          code = code line_code "\n"
          if (string_mode == "quoted") {
            string_escape = 0
          }
        }
        function count_group_suppressions(text, start, end,    i, j, token, after, total) {
          total = 0
          for (i = start; i <= end;) {
            if (substr(text, i, 1) ~ /[[:alpha:]_]/) {
              j = i + 1
              while (j <= end && substr(text, j, 1) ~ /[[:alnum:]_]/) {
                j++
              }
              token = substr(text, i, j - i)
              if (token == "allow" || token == "expect") {
                after = j
                while (after <= end && substr(text, after, 1) ~ /[[:space:]]/) {
                  after++
                }
                if (substr(text, after, 1) == "(" && !counted_suppressions[i]) {
                  counted_suppressions[i] = 1
                  total++
                }
              }
              i = j
            } else {
              i++
            }
          }
          return total
        }
        function count_attribute_suppressions(text,    total, i, j, k, depth, start, end, char, n) {
          total = 0
          n = length(text)
          for (i = 1; i <= n; i++) {
            if (substr(text, i, 1) != "#") {
              continue
            }
            j = i + 1
            while (j <= n && substr(text, j, 1) ~ /[[:space:]]/) {
              j++
            }
            if (substr(text, j, 1) == "!") {
              j++
              while (j <= n && substr(text, j, 1) ~ /[[:space:]]/) {
                j++
              }
            }
            if (substr(text, j, 1) != "[") {
              continue
            }

            start = j + 1
            depth = 1
            for (k = start; k <= n && depth > 0; k++) {
              char = substr(text, k, 1)
              if (char == "[") {
                depth++
              } else if (char == "]") {
                depth--
              }
            }
            if (depth == 0) {
              end = k - 2
              total += count_group_suppressions(text, start, end)
            }
          }
          return total
        }
        END {
          if (failed) {
            exit 2
          }
          if (block_comment_depth > 0) {
            printf "ratchet-count.sh: nicht abgeschlossener Blockkommentar in %s:%d\n", FILENAME, NR > "/dev/stderr"
            exit 2
          }
          if (string_mode != "") {
            printf "ratchet-count.sh: nicht abgeschlossenes String-Literal in %s:%d\n", FILENAME, NR > "/dev/stderr"
            exit 2
          }
          print count_attribute_suppressions(code)
        }
      ' "$file"); then
        printf 'ratchet-count.sh: Attribut-Scan fehlgeschlagen: %s\n' "$file" >&2
        exit 2
      fi
      count=$((count + matches))
    done < "$tmp"
    printf '%s\n' "$count"
    ;;

  *)
    usage_error "unbekanntes oder fehlendes Unterkommando: ${1-}"
    ;;
esac
