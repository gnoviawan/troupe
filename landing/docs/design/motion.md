# Motion

CSS only (transitions and keyframes). There is no motion library; do not add one.

```
Should it move?
 ├── Is it on the page when it loads? → no entrance animation
 ├── Was it caused by the keyboard (arrow keys, shortcut)? → no animation
 ├── Is it a hover? → colour only, 150ms
 └── Caused by a pointer or by the tour → animate transform and opacity, curve and duration below
```

## Values

| Part | Duration | Curve | What moves |
|---|---|---|---|
| Press (primary, secondary button) | 150ms | `--ease-out` | `scale: .97` on `:active` (the `.press` class in `Button`) |
| Hover, row highlight | 150ms | `--ease-out` | colour only |
| Menu open / close | 150ms / 120ms | `--ease-out` | opacity + `scale` from .97, origin at the middle of the trigger |
| Tour tab panel | 200ms in, 150ms out | `--ease-out` | opacity (cross-fade in one grid cell) |
| Tour tab pill | 300ms | `--ease-in-out` | `translate` + width (UI SDK `--active-tab-*`) |
| Tour progress | one tab (8.8s) | linear | `scale` x |
| Tour scene parts | 200–300ms appear, 150ms leave | `--ease-out` | opacity + 4px rise or scale .97 |
| Tour scene moves across the window | 400–500ms | `--ease-in-out` | transform |
| FAQ answer | 250ms | `--ease-out` | height (measured by UI SDK) |
| Card illustration loops | 3.2–6.4s | per Paper keyframes | transform, opacity, stroke-dashoffset |

- Only `transform`, `opacity` (and `stroke-dashoffset` in SVG). The listed measured heights and the tab pill width are the only layout animations; do not add more.
- Exits are faster than entrances. Never start from `scale(0)`; never `ease-in`; never `transition-all`.
- Menus are transitions (they can reopen mid-way), never keyframes.
- Never a permanent `will-change`.

```tsx
// Correct — src/components/download/DownloadMenu.tsx
'transition-[opacity,scale] duration-150 ease-(--ease-out)',
'data-starting-style:scale-[0.97] data-starting-style:opacity-0',
'data-ending-style:scale-[0.97] data-ending-style:opacity-0 data-ending-style:duration-120',

// Incorrect — animates everything, weak curve, starts from nothing
'transition-all duration-300 ease-in data-starting-style:scale-0'
```

## The tour

- Auto-advance stops on mouse hover, keyboard focus inside, off screen (less than 25% visible), a hidden browser tab, reduced motion, and the Pause button. Keep all six when you change it.
- Loops inside a scene pause with the tour (`.tour-stage[data-running=false]`). A new loop needs nothing extra if it is a CSS animation inside the stage.

## Reduced motion

Every file with motion has a `prefers-reduced-motion: reduce` path: no movement, opacity changes at most 200ms, loops stopped, the tour shows its final frame and does not advance. In Tailwind: `motion-reduce:transition-none` or `motion-reduce:transition-opacity` plus `motion-reduce:data-starting-style:scale-100`. Never a base state of `opacity: 0` that only the animation clears.
