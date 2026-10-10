#!/bin/sh
# aski end-to-end test suite.
#
# usage: sh tests/e2e.sh [popup|server|all]     (default: all)
#
# popup  — interactive: a window appears and YOU answer it as instructed;
#          the script then checks the JSON the binary printed on stdout.
# server — non-interactive: MCP validation errors over stdio, no windows.

set -u

cd "$(dirname "$0")/.." || exit 1

PASS=0
FAIL=0
STDERR_TMP=$(mktemp)
trap 'rm -f "$STDERR_TMP"' EXIT

say()  { printf '\n\033[1m== %s ==\033[0m\n' "$1"; }
hint() { printf '     \033[33m-> %s\033[0m\033[0m\n' "$1"; }

check() { # check <name> <expected substring> <actual output>
    name=$1
    expected=$2
    actual=$3
    case "$actual" in
        *"$expected"*)
            printf 'PASS  %s\n' "$name"
            PASS=$((PASS + 1))
            ;;
        *)
            printf 'FAIL  %s\n' "$name"
            printf '      expected substring: %s\n' "$expected"
            printf '      actual: %s\n' "$actual"
            if [ -s "$STDERR_TMP" ]; then
                printf '      stderr (last 5 lines):\n'
                tail -n 5 "$STDERR_TMP" | sed 's/^/        /'
            fi
            FAIL=$((FAIL + 1))
            ;;
    esac
}

popup_test() { # popup_test <name> <expected substring> <spec-json>
    name=$1
    expected=$2
    json=$3
    : > "$STDERR_TMP"
    actual=$(printf '%s\n' "$json" | cargo run --quiet -- popup 2>"$STDERR_TMP")
    check "$name" "$expected" "$actual"
}

serve_call() { # serve_call <id> <arguments-json> <hold-seconds>
    id=$1
    args=$2
    hold=$3
    {
        printf '%s\n' \
            '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"e2e","version":"0"}}}' \
            '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
            "{\"jsonrpc\":\"2.0\",\"id\":$id,\"method\":\"tools/call\",\"params\":{\"name\":\"ask_user\",\"arguments\":$args}}"
        sleep "$hold"
    } | cargo run --quiet -- serve 2>"$STDERR_TMP"
}

# ---------------------------------------------------------------- popup tests

t1_single_select() {
    say "T1: single-select"
    hint "click 'Rust' (it must highlight), then press Next"
    popup_test "T1 single-select click+Next" '"selections":["Rust"]' \
        '{"questions":[{"question":"Click Rust, then Next.","header":"T1","options":[{"label":"Rust","description":"memory safe"},{"label":"C","description":"fast"}]}],"timeout_secs":120}'
}

t2_keyboard() {
    say "T2: keyboard navigation"
    hint "press ArrowDown twice, then Enter (selects SQLite), then Shift+Enter to send"
    popup_test "T2 arrow keys + Enter" '"selections":["SQLite"]' \
        '{"questions":[{"question":"Press Down twice, then Enter.","header":"T2","options":[{"label":"PostgreSQL"},{"label":"SQLite"},{"label":"MongoDB"}]}],"timeout_secs":120}'
}

t3_multi_select() {
    say "T3: multi-select"
    hint "tick 'Structured logging' and 'CI/CD pipeline', then Next"
    popup_test "T3 multi-select ticks" '"selections":["Structured logging","CI/CD pipeline"]' \
        '{"questions":[{"question":"Tick two features.","header":"T3","options":[{"label":"Automated tests"},{"label":"Structured logging"},{"label":"Performance metrics"},{"label":"CI/CD pipeline"}],"multi_select":true}],"timeout_secs":120}'
}

t4_other() {
    say "T4: free-form answer"
    hint "type 'my own answer' into Other (Enter adds lines only), then press Shift+Enter to send"
    popup_test "T4 Other free-form" '"selections":["my own answer"]' \
        '{"questions":[{"question":"Type a custom answer in the Other field.","header":"T4","options":[{"label":"A"},{"label":"B"}]}],"timeout_secs":120}'
}

t5_preview() {
    say "T5: preview panel"
    hint "move Down between options — a code panel must appear for PostgreSQL/SQLite; answer SQLite (Enter selects), then Shift+Enter to send"
    popup_test "T5 preview panel" '"selections":["SQLite"]' \
        '{"questions":[{"question":"Check the preview panel, then answer SQLite.","header":"T5","options":[{"label":"PostgreSQL","preview":"DATABASE_URL=postgres://localhost/app"},{"label":"SQLite","preview":"Connection::open(\"app.db\")?"},{"label":"MongoDB"}]}],"timeout_secs":120}'
}

t6_navigation() {
    say "T6: multi-question navigation"
    hint "Next to Q2, Back to Q1, pick C, Next, pick B on Q2, Next = Submit"
    popup_test "T6 back navigation + per-question answers" '"answers":[{"header":"Q1","selections":["C"]},{"header":"Q2","selections":["B"]}]' \
        '{"questions":[{"question":"Q1: pick C (you will come back to it).","header":"Q1","options":[{"label":"A"},{"label":"B"},{"label":"C"}]},{"question":"Q2: pick B.","header":"Q2","options":[{"label":"A"},{"label":"B"}]}],"timeout_secs":180}'
}

t7_cancel() {
    say "T7: cancel"
    hint "press Esc (or close the window)"
    popup_test "T7 Esc cancel" '"status":"cancelled"' \
        '{"questions":[{"question":"Press Esc to cancel.","header":"T7","options":[{"label":"A"},{"label":"B"}]}],"timeout_secs":120}'
}

t8_timeout() {
    say "T8: timeout"
    hint "do NOT touch the window — 5s countdown must tick down and self-close"
    popup_test "T8 timeout countdown" '"status":"timeout"' \
        '{"questions":[{"question":"Wait without touching anything.","header":"T8","options":[{"label":"A"},{"label":"B"}]}],"timeout_secs":5}'
}

t14_long_note() {
    say "T14: long multiline note"
    hint "pick Rust, then paste/type this 4-line note (Enter = new lines):"
    hint "  line one with Polish chars: abcdefg"
    hint "  aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    hint "  line after a short unbroken token"
    hint "  END-OF-NOTE"
    hint "then press Shift+Enter to send"
    popup_test "T14 long note appended after selection" '"selections":["Rust","line one' \
        '{"questions":[{"question":"Pick Rust and paste the long note from the hint.","header":"T14","options":[{"label":"Rust"},{"label":"C"}]}],"timeout_secs":180}'
    check "T14 long note fully captured" 'END-OF-NOTE"' "$actual"
}

t15_long_options() {
    say "T15: long option labels"
    hint "both options wrap over several lines — the window must fit everything;"
    hint "pick the Rust option, then Next"
    popup_test "T15 long options fit + full label returned" '"selections":["Rust — memory safety without a garbage collector' \
        '{"questions":[{"question":"Pick the Rust option (labels are intentionally long — check the window fits them all).","header":"T15","options":[{"label":"Rust — memory safety without a garbage collector, zero-cost abstractions and fearless concurrency","description":"Choice A: compiled and predictable performance; the ownership model prevents data races at compile time instead of runtime crashes."},{"label":"C — manual memory management with malloc and free, where every dangling pointer and buffer overflow is entirely your responsibility","description":"Choice B: maximum control and a tiny runtime, but use-after-free bugs are all yours to find."}]}],"timeout_secs":120}'
}

t9_e2e_serve() {
    say "T9: end-to-end through aski serve"
    hint "click 'Works', then Next — goes through the real MCP stdio path"
    out=$(serve_call 2 '{"questions":[{"question":"End-to-end: click Works, then Next.","header":"E2E","options":[{"label":"Works","description":"full MCP path"},{"label":"Nope"}]}],"timeout_secs":90}' 95)
    check "T9 serve e2e answered" '"status":"answered"' "$out"
    check "T9 serve e2e per-question answers" '"header":"E2E"' "$out"
    check "T9 serve e2e selections copy" '"selections":["Works"]' "$out"
}

# --------------------------------------------------------------- server tests

t10_too_many_questions() {
    say "T10: validation — 5 questions"
    out=$(serve_call 2 '[{"question":"a","options":[{"label":"1"},{"label":"2"}]},{"question":"b","options":[{"label":"1"},{"label":"2"}]},{"question":"c","options":[{"label":"1"},{"label":"2"}]},{"question":"d","options":[{"label":"1"},{"label":"2"}]},{"question":"e","options":[{"label":"1"},{"label":"2"}]}]' 1)
    check "T10 1-4 questions limit" 'questions must contain between 1 and 4 entries' "$out"
}

t11_too_many_options() {
    say "T11: validation — 5 options"
    out=$(serve_call 3 '[{"question":"x","options":[{"label":"1"},{"label":"2"},{"label":"3"},{"label":"4"},{"label":"5"}]}]' 1)
    check "T11 2-4 options limit" 'options must contain between 2 and 4 entries' "$out"
}

t12_empty_question() {
    say "T12: validation — empty question"
    out=$(serve_call 4 '[{"question":"   ","options":[{"label":"1"},{"label":"2"}]}]' 1)
    check "T12 empty question rejected" 'question must not be empty' "$out"
}

t13_empty_label() {
    say "T13: validation — empty option label"
    out=$(serve_call 5 '[{"question":"x","options":[{"label":"  "},{"label":"2"}]}]' 1)
    check "T13 empty label rejected" 'non-empty label' "$out"
}

# ------------------------------------------------------------------- runner

usage() {
    printf 'usage: sh tests/e2e.sh [popup|server|all]\n'
    exit 2
}

GROUP=${1:-all}

case "$GROUP" in
    popup)
        say "building"
        cargo build --quiet || exit 1
        t1_single_select
        t2_keyboard
        t3_multi_select
        t4_other
        t5_preview
        t6_navigation
        t7_cancel
        t8_timeout
        t14_long_note
        t15_long_options
        t9_e2e_serve
        ;;
    server)
        say "building"
        cargo build --quiet || exit 1
        t10_too_many_questions
        t11_too_many_options
        t12_empty_question
        t13_empty_label
        ;;
    all)
        say "building"
        cargo build --quiet || exit 1
        t1_single_select
        t2_keyboard
        t3_multi_select
        t4_other
        t5_preview
        t6_navigation
        t7_cancel
        t8_timeout
        t14_long_note
        t15_long_options
        t9_e2e_serve
        t10_too_many_questions
        t11_too_many_options
        t12_empty_question
        t13_empty_label
        ;;
    *)
        usage
        ;;
esac

printf '\n%s\n' "================================"
printf '%d passed, %d failed\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ]
