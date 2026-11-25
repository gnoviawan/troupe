#!/usr/bin/env bash
# The R7b box-drawing screen: light, double, heavy and rounded boxes with a wide cell inside,
# block elements, shades and diagonals, so joined lines show against the font's gaps.
printf '┌──────┬──────┐  ╔══════╦══════╗  ╭──────╮\n'
printf '│ one  │ two  │  ║ one  ║ two  ║  │ cur  │\n'
printf '├──────┼──────┤  ╠══════╬══════╣  ╰──────╯\n'
printf '│ 三   │ four │  ║ 三   ║ four ║  ┏━━┳━━┓\n'
printf '└──────┴──────┘  ╚══════╩══════╝  ┗━━┻━━┛\n'
printf '█▀▄▌▐░▒▓ ▁▂▃▄▅▆▇█ ▖▗▘▝▚▞ ╱╲╳\n'
