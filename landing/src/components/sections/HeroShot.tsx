import { Container } from './Container';

/**
 * The app window over the dusk photo. One static WebP per layout:
 * desktop is the full window, phone is a crop of the sidebar and composer.
 */
export function HeroShot() {
  return (
    <Container>
      <picture>
        <source
          media="(min-width: 768px)"
          srcSet="/media/hero-desktop.webp 1x, /media/hero-desktop@2x.webp 2x"
          width={1248}
          height={780}
        />
        <img
          src="/media/hero-mobile.webp"
          srcSet="/media/hero-mobile.webp 1x, /media/hero-mobile@2x.webp 2x"
          width={342}
          height={320}
          alt="Troupe on macOS: the sidebar lists projects and chats with Claude Code, Codex, Gemini CLI, OpenCode and Cursor, and the composer asks “What should we do in Troupe?”"
          fetchPriority="high"
          decoding="async"
          className="h-80 w-full rounded-xl border-[0.5px] border-border bg-[#0B0C0D] object-cover object-left-top md:h-auto"
        />
      </picture>
    </Container>
  );
}
