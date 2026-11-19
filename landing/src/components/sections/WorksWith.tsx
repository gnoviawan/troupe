import { AGENTS, AgentLogo, type AgentId } from '@/components/ui/AgentLogo';
import { Container } from './Container';

const LIST: AgentId[] = ['claude-code', 'codex', 'gemini-cli', 'antigravity', 'opencode', 'cursor'];

export function WorksWith() {
  return (
    <section id="agents" aria-labelledby="works-with-title" className="scroll-mt-6">
      <Container>
        <div className="flex flex-col gap-4 border-b-[0.5px] border-border py-10 md:flex-row md:items-center md:justify-between md:py-12">
          <h2 id="works-with-title" className="text-sm leading-5 font-medium text-muted-foreground md:text-[13px]">
            Works with the agents you use
          </h2>
          <ul className="flex flex-wrap items-center gap-x-6 gap-y-[18px] lg:gap-x-10">
            {LIST.map((id) => (
              <li key={id} className="flex items-center gap-2">
                <AgentLogo agent={id} size={20} />
                <span className="text-base leading-6 font-semibold tracking-[-0.01em] text-[#B6B7B9]">{AGENTS[id].name}</span>
              </li>
            ))}
            <li className="text-sm leading-5 font-medium tracking-[-0.01em] text-muted-foreground">+ any ACP agent</li>
          </ul>
        </div>
      </Container>
    </section>
  );
}
