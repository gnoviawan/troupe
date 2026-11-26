#!/usr/bin/env bash
# The R7b selection, search and link screen: one long line the terminal wraps, three lines
# with "needle", and a line with a URL.
printf 'word%.0s ' $(seq 1 60); echo
echo 'needle one'
echo 'two needle'
echo 'needle three'
echo 'see https://example.com/troupe?x=1 ok'
