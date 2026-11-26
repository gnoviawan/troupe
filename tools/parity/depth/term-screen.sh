#!/usr/bin/env bash
# The colour, attribute and box-drawing screen the terminal cell recordings print
# (`term-record.py`). Plain printf, so its bytes do not depend on the host.
e=$'\e'
printf '16 colours  '
for c in 0 1 2 3 4 5 6 7; do printf '%s[3%sm%s %s[0m' "$e" "$c" "$c" "$e"; done
for c in 0 1 2 3 4 5 6 7; do printf '%s[9%sm%s %s[0m' "$e" "$c" "$c" "$e"; done
printf '\n'
for c in 0 1 2 3 4 5 6 7; do printf '%s[4%sm  %s[0m' "$e" "$c" "$e"; done
for c in 0 1 2 3 4 5 6 7; do printf '%s[10%sm  %s[0m' "$e" "$c" "$e"; done
printf '\n256: '
for c in $(seq 16 4 255); do printf '%s[48;5;%sm %s[0m' "$e" "$c" "$e"; done
printf '\nrgb: '
for i in $(seq 0 4 255); do printf '%s[48;2;%s;%s;%sm %s[0m' "$e" "$i" "$((255 - i))" "$((i / 2))" "$e"; done
printf '\n'
printf '%s[1mbold%s[0m %s[2mdim%s[0m %s[3mitalic%s[0m %s[4munder%s[0m %s[5mblink%s[0m ' "$e" "$e" "$e" "$e" "$e" "$e" "$e" "$e" "$e" "$e"
printf '%s[7minverse%s[0m %s[8mhidden%s[0m %s[9mstrike%s[0m %s[53mover%s[0m %s[4:3mcurly%s[0m\n' "$e" "$e" "$e" "$e" "$e" "$e" "$e" "$e" "$e" "$e"
printf '%s[1;31;44mbold red on blue%s[22m normal%s[39;49m default %s[38:5:208mcolon 256%s[38:2::10:200:30m colon rgb%s[m\n' "$e" "$e" "$e" "$e" "$e" "$e"
printf '┌──────┬──────┐ ╔══╗ ▀▄█▌▐░▒▓\n'
printf '│ cell │ cell │ ║  ║ ⠁⠃⠇⡇⣇⣧⣷⣿\n'
printf '├──────┼──────┤ ╚══╝ ╭─╮╰─╯\n'
printf '└──────┴──────┘ %s(0lqqk x x mqqj%s(B ascii again\n' "$e" "$e"
printf '0123456789012345678901234567890\n'
printf '中文字符|wide 한국어|日本\n'
printf 'e\xcc\x81 combining a\xcc\x8a  😀 emoji ✅ check\n'
printf 'tabs:\tone\ttwo\tthree\n'
printf '%s[41mbce erase%s[K%s[0m\n' "$e" "$e" "$e"
