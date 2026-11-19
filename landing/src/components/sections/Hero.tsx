import { DownloadCta } from '@/components/download/DownloadCta';
import { Button } from '@/components/ui/Button';
import { GitHubIcon } from '@/components/ui/Icons';
import { GITHUB_REPO_URL } from '@/lib/links';
import { Container } from './Container';

export function Hero() {
  return (
    <section id="download" aria-labelledby="hero-title" className="scroll-mt-6">
      <Container className="pt-12 pb-10 md:pt-[120px] md:pb-[72px]">
        <div className="flex max-w-[680px] flex-col gap-5 md:gap-6">
          <h1
            id="hero-title"
            className="text-[40px] leading-11 font-medium tracking-[-0.03em] text-foreground md:text-[56px] md:leading-[60px]"
          >
            Any Agents <br className="md:hidden" />
            in One UI
          </h1>
          <p className="text-base leading-6 text-muted-foreground md:text-lg md:leading-7">
            Run every coding agent from one desktop app. <br className="hidden md:inline" />
            Plan, review and ship with a troupe of agents, from your desk or phone.
          </p>
          <div className="flex flex-col gap-2.5 pt-2 md:flex-row md:pt-3">
            <DownloadCta />
            <Button
              variant="secondary"
              size="lg"
              className="h-11 w-full px-4 text-base md:w-auto md:px-8 md:text-[15px]"
              render={<a href={GITHUB_REPO_URL} />}
            >
              <GitHubIcon size={16} />
              View on GitHub
            </Button>
          </div>
          <p className="text-center text-sm leading-5 text-muted-foreground md:text-left md:text-[13px]">
            Also for Windows and Linux
            <span className="hidden md:inline"> · </span>
            <br className="md:hidden" />
            Free and open source, MIT
          </p>
        </div>
      </Container>
    </section>
  );
}
