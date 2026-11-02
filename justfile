set dotenv-load

channel := "1365521515356885032"
strats := "data/export/strats.json"
# AUR and nixpkgs name the binary differently
exporter := `command -v discord-chat-exporter-cli || echo discordchatexporter-cli`

_default:
	just --list

# check out the data branch as a separate workspace in data/
setup-workspace:
    jj bookmark track data@origin
    jj workspace root --name data >/dev/null 2>&1 || jj workspace add data -r data

# fetch messages newer than the last one in the export, then regenerate data
update: && data
    #!/usr/bin/env bash
    set -euo pipefail
    # the exporter reads DISCORD_TOKEN from the environment
    last=$(jq -r '.messages | max_by(.id | [length, .]) | .id' {{strats}})
    tmp=$(mktemp -d)
    trap 'rm -rf "$tmp"' EXIT
    {{exporter}} export -c {{channel}} -f Json --after "$last" -o "$tmp/new.json"
    # merge by message id, newer export wins; ids are snowflakes beyond float precision, so compare as (length, string)
    jq -s '.[0] + {
        messages: ((.[0].messages + .[1].messages) | group_by(.id) | map(last) | sort_by(.id | [length, .])),
        exportedAt: .[1].exportedAt,
      } | .messageCount = (.messages | length)' {{strats}} "$tmp/new.json" > "$tmp/merged.json"
    echo "added $(jq '.messages | length' "$tmp/new.json") messages after $last"
    mv "$tmp/merged.json" {{strats}}

# re-export the whole channel
export-all: && data
    {{exporter}} export -c {{channel}} -f Json -o {{strats}}

# strats.json, map.json and media previews
data:
    cargo run --release

# re-sign all attachment links of data/strats.json into out (they expire after ~24h); needs DISCORD_BOT_TOKEN
refresh out="data/strats.json":
    #!/usr/bin/env bash
    set -euo pipefail
    data=data/strats.json
    tmp=$(mktemp -d)
    trap 'rm -rf "$tmp"' EXIT
    echo '[]' > "$tmp/refreshed.json"
    jq -c '[.posts[].attachments[].url | sub("\\?.*"; "")] | unique | [range(0; length; 50) as $i | .[$i:$i + 50]][]' "$data" |
    while read -r batch; do
        curl -sS --fail-with-body -X POST https://discord.com/api/v10/attachments/refresh-urls \
            -H "Authorization: Bot $DISCORD_BOT_TOKEN" -H "Content-Type: application/json" \
            -H "User-Agent: DiscordBot (hk-stratlist, 0.1)" \
            --data "{\"attachment_urls\": $batch}" > "$tmp/batch.json"
        jq -s '.[0] + .[1].refreshed_urls' "$tmp/refreshed.json" "$tmp/batch.json" > "$tmp/merged.json"
        mv "$tmp/merged.json" "$tmp/refreshed.json"
    done
    jq --slurpfile r "$tmp/refreshed.json" '
        ($r[0] | map({key: (.original | sub("\\?.*"; "")), value: .refreshed}) | from_entries) as $fresh
        | .posts[].attachments[] |= (.url = ($fresh[.url | sub("\\?.*"; "")]
            // error("not refreshed: \(.url)")))' "$data" > "$tmp/data.json"
    mkdir -p "$(dirname "{{out}}")"
    mv "$tmp/data.json" "{{out}}"
    echo "refreshed $(jq length "$tmp/refreshed.json") attachment links"

# push the data branch and rebuild the site
publish:
    jj -R data git push -b data
    gh workflow run pages.yml

# dev server, on a copy of the data with fresh links so the data workspace stays untouched
dev: (refresh "web/.dev-data/strats.json")
    cd web && STRATS_JSON=.dev-data/strats.json pnpm dev

# final export
build: data
    cd web && pnpm build

preview: build
    cd web && pnpm preview
