# Window shots

The page shows real Troupe UI. Two ways to show it:

```
Does the UI move, reflow on a phone, or need sharp text at any size?
 ├── Yes → code it with app-ui parts (tour scenes, bento menus and lists, card illustrations)
 └── No → a WebP exported from Paper at 1x and 2x, desktop and phone crops, in <picture> (hero shot, bento Agents and Phone tiles)
```

## The photo stage

Every mock window sits on a dithered photo: `<Stage background="dusk | pastel | green | orange">` (`app-ui/Window.tsx`). Inside it, `<AppWindow>` at `top-10 left-10` (40px in), 900×700 or 900×640, clipped by the stage. Never place a mock window on a plain background or without the clip.

The photos are `public/media/bg-*.webp` (licensed for this use). For a section background outside a stage, use the `.dither` class with `--dither-image` (see `ClosingCta`).

## Mock UI parts (`src/components/app-ui/`)

`Composer`, `PopoverMenu` + `MenuLabel` + `MenuRow` + `MenuSeparatorGroup`, `Switch`, `McpLogo`, `Stage`, `AppWindow`, and the stroke icons. Tour scenes also use `tour/app-ui.tsx` (`WindowTabBar`, `WindowTab`, `TrafficLights`).

- Mock UI is decorative: the stage is `aria-hidden`, its controls are `<span>`/`<div>`, never `<button>` or `<a>`. A real control inside a picture would be a dead tab stop.
- Mock text sizes copy Paper exactly (11–13px is normal here); the phone text floor does not apply inside a picture.
- Hex values from Paper are allowed in mock UI (`#373839`, `#3ECF8E` selection green) with a comment, because the app draws them so.
- Mock data: demo projects, "My Mac", "Personal" account, Logoipsum marks. Never real private names.

## Feature tour (`src/components/tour/`)

- A tab is a `TourTab` (`tour/types.ts`) in `src/data/tour.tsx`: copy (title, description, 4 steps, optional footer) from the Paper frames "Feature tour · All states (full stage)", a `background`, a `mobileFocus` (stage px of the 342×330 phone crop's top-left), an optional `frame: 'none'`, and a `Scene`.
- A tab file in `tour/tabs/` exports only components (its `Scene`, and helpers like `AccountsFooter`). The tab data lives in `src/data/tour.tsx`, so Fast Refresh keeps state.
- A `Scene({ step })` renders all 4 keyframes at once and switches parts with CSS on `data-step` (0–3). No timers or JS animation in a scene.
- Step 4 is the reduced-motion still: it must match Paper 4/4 exactly.
- Pick `mobileFocus` so the crop shows the key UI of the last step.

## Card illustrations (`src/components/illustrations/`)

Inline SVG from Paper ("More in one place" cards), grey hairlines only, no fills and no colour. Loops run only while the card is on screen (`useInView` → `playing`). The paused frame is the Paper still. A new illustration follows the same rules; never a stock image or an emoji.
