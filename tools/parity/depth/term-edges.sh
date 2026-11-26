#!/usr/bin/env bash
# Edge cases of the terminal cell recordings (`term-record.py`): wrapping, wide
# cells at the right edge, insert and delete, scroll regions, saved cursors and
# the cursor style. It assumes an 80-column screen.
e=$'\e'
printf '%s[2J%s[H' "$e" "$e"
printf 'wrap: %s\n' "$(printf 'abcdefghij%.0s' 1 2 3 4 5 6 7 8 9)"
printf '%s[78G中x\n' "$e"
printf '%s[79G日本\n' "$e"
printf 'insert: 0123456789%s[5D%s[2@++%s[4h**%s[4l\n' "$e" "$e" "$e" "$e"
printf 'delete: 0123456789%s[6D%s[3P\n' "$e" "$e"
printf 'erase:  0123456789%s[6D%s[3X|%s[1K\n' "$e" "$e" "$e"
printf 'wide erase: 中文中文%s[3G%s[X\n' "$e" "$e"
printf 'rep: x%s[5b|\n' "$e"
printf 'line A\nline B\nline C\nline D\n%s[3A%s[L%s[1M%s[3B\n' "$e" "$e" "$e" "$e"
printf '%s7%s[5;1Hsaved and restored%s8after restore\n' "$e" "$e" "$e"
printf '%s[44m%s[16;20r%s[20;1Hregion 1\nregion 2\nregion 3\nregion 4\nregion 5\n%s[16;1H%sMtop insert%s[r%s[0m' "$e" "$e" "$e" "$e" "$e" "$e" "$e"
printf '%s[22;1H%s[?6h%s[2;3r%s[1;1Horigin%s[?6l%s[r' "$e" "$e" "$e" "$e" "$e" "$e"
printf '%s[23;1H%s[3g%s[11G%sH%s[21G%sH%s[31G%sH%s[21G%s[0g%s[1G\ttab\tstops\n' "$e" "$e" "$e" "$e" "$e" "$e" "$e" "$e" "$e" "$e" "$e"
printf '%s[?1049h%s[2Jalt screen text%s[?1049l' "$e" "$e" "$e"
printf '%s[24;1Hdone%s[5 q' "$e" "$e"
