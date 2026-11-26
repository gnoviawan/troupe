import type { Surface } from '../types'
import {
  CHAT_SETTLED,
  click,
  css,
  DASHBOARD,
  id,
  label,
  openChat,
  see,
  step,
  txt,
  wait
} from './dsl'

const NO_OPTIONS =
  'needs an agent that offers models and config options; the scripted agent offers none'
const NO_SUBAGENT = 'needs a running sub-agent, which the scripted agent never starts'
const NO_IMAGE = 'needs an image in the chat, and the scripted agent sends none'
const LAUNCHER_FALLBACK =
  'the launcher overlay opens only as a fallback (no agent set up, or a sign-in); New chat opens an empty chat'
const NO_CLAUDE =
  'needs a Claude agent signed in with accounts, which the seeded world does not hold'
const NO_CODEX = 'needs a Codex agent signed in with accounts, which the seeded world does not hold'
const chatMenu = click(css('button[aria-label^="More actions for How does"]'))

export const CHAT: Surface[] = [
  {
    id: 'chat.transcript',
    area: 'chat',
    title: 'Open a chat: transcript, tool cards, result',
    source: { screen: 'pane', name: 'agent-chat' },
    roi: '[data-slot="message-scroller-viewport"]',
    steps: [
      DASHBOARD,
      step(
        'b-open',
        [openChat, wait(1500)],
        [
          ...see('Where else is the chat route parsed?', 'One more place parses it'),
          // The scroller's button text: gone once the transcript rests at its end.
          { text: 'Scroll to latest', gone: true, timeoutMs: 15_000 }
        ]
      )
    ]
  },
  {
    id: 'chat.agent-launcher',
    area: 'chat',
    title: 'Agent and model menu',
    source: {
      overlay: [
        'components/chat/agent-model-selector/AgentModelSelector.tsx',
        'components/chat/agent-model-selector/mobile/MobileModelSheet.tsx'
      ]
    },
    steps: [
      DASHBOARD,
      step(
        'b-menu',
        [click(id('agent-model-selector-trigger'))],
        see('This agent gives no model choice.')
      )
    ]
  },
  {
    id: 'chat.table-zoom',
    area: 'chat',
    title: 'Markdown table full screen',
    source: { overlay: ['components/chat/chat-markdown-table.tsx'] },
    steps: [
      DASHBOARD,
      step('b-open', [openChat, wait(1500)], CHAT_SETTLED),
      step(
        'c-zoom',
        [click(css('[data-streamdown="table-toolbar"] button[aria-label="View fullscreen"]'))],
        see('Table')
      )
    ]
  },
  {
    id: 'chat.delete-confirm',
    area: 'chat',
    title: 'Delete chat confirm',
    source: { overlay: ['components/chat/ChatHistoryTab.tsx'] },
    steps: [
      DASHBOARD,
      step('b-menu', [chatMenu], see('Delete chat')),
      step('c-confirm', [click(txt('Delete chat'))], see('Cancel'))
    ]
  },
  {
    id: 'chat.import',
    area: 'chat',
    title: 'Import chats dialog',
    source: { overlay: ['components/chat/ImportChatsDialog.tsx'] },
    steps: [
      DASHBOARD,
      step(
        'b-dialog',
        [click(label('Add project or folder group')), click(txt('Import chats', false))],
        see('Import chats')
      )
    ]
  },
  {
    id: 'chat.reload-confirm',
    area: 'chat',
    title: 'Reload chat confirm',
    source: { overlay: ['components/chat/ReloadChatConfirmDialog.tsx'] },
    blocked: 'opens only when the host asks to reload a chat that has unsent work',
    steps: []
  },
  {
    id: 'chat.image-lightbox',
    area: 'chat',
    title: 'Chat image lightbox',
    source: { overlay: ['components/chat/ChatMedia.tsx'] },
    blocked: NO_IMAGE,
    steps: []
  },
  {
    id: 'chat.image-gallery',
    area: 'chat',
    title: 'Chat image gallery',
    source: { overlay: ['components/chat/ChatMedia.tsx'] },
    blocked: NO_IMAGE,
    steps: []
  },
  {
    id: 'chat.attachment-viewer',
    area: 'chat',
    title: 'Full-screen attachment image viewer',
    source: { overlay: ['components/chat/AttachmentImageViewer.tsx'] },
    blocked: 'needs an image attached in the composer; the seeded world has no image file',
    steps: []
  },
  {
    id: 'chat.attachment-hover',
    area: 'chat',
    title: 'Attachment hover card',
    source: { overlay: ['components/ai-elements/attachments.tsx'] },
    blocked: 'needs an image attached in the composer; the seeded world has no image file',
    steps: []
  },
  {
    id: 'chat.context-usage',
    area: 'chat',
    title: 'Context usage menu',
    source: {
      overlay: [
        'components/chat/ContextUsageIndicator.tsx',
        'components/chat/ContextUsageIndicator.tsx'
      ]
    },
    blocked: 'needs an agent that reports context usage; the scripted agent reports none',
    steps: []
  },
  {
    id: 'chat.status-chip',
    area: 'chat',
    title: 'Status rail segment panel',
    source: { overlay: ['components/chat/StatusChip.tsx', 'components/chat/StatusChip.tsx'] },
    blocked: 'needs sub-agents, background work or changed files in the chat',
    steps: []
  },
  {
    id: 'chat.subagent-stop',
    area: 'chat',
    title: 'Stop sub-agent confirm',
    source: { overlay: ['components/chat/SubagentRowStop.tsx'] },
    blocked: NO_SUBAGENT,
    steps: []
  },
  {
    id: 'chat.subagent-sheet',
    area: 'chat',
    title: 'Sub-agent panel sheet',
    source: { overlay: ['components/chat/SubagentSurface.tsx'] },
    blocked: NO_SUBAGENT,
    steps: []
  },
  {
    id: 'chat.ask-user',
    area: 'chat',
    title: 'Ask-user question card',
    source: { overlay: ['components/chat/AskUserQuestion.tsx'] },
    blocked: 'needs an agent question; the scripted agent asks none',
    steps: []
  },
  {
    id: 'chat.elicitation',
    area: 'chat',
    title: 'Elicitation prompt',
    source: { overlay: ['components/chat/ElicitationPrompt.tsx'] },
    blocked: 'needs an MCP elicitation; the scripted agent sends none',
    steps: []
  },
  {
    id: 'chat.mcp-install',
    area: 'chat',
    title: 'MCP install approval card',
    source: { overlay: ['components/chat/McpInstallCard.tsx'] },
    blocked: 'needs an agent mcp_install request; the scripted agent sends none',
    steps: []
  },
  {
    id: 'chat.header-selector-modal',
    area: 'chat',
    title: 'Chat header selector (phone modal)',
    source: { overlay: ['components/chat/AgentHeader.tsx'] },
    profile: 'phone',
    blocked: NO_OPTIONS,
    steps: []
  },
  {
    id: 'chat.header-model-menu',
    area: 'chat',
    title: 'Chat header model menu',
    source: { overlay: ['components/chat/AgentHeader.tsx'] },
    blocked: NO_OPTIONS,
    steps: []
  },
  {
    id: 'chat.header-option-menu',
    area: 'chat',
    title: 'Chat header config option menu',
    source: { overlay: ['components/chat/AgentHeader.tsx'] },
    blocked: NO_OPTIONS,
    steps: []
  },
  {
    id: 'chat.selector-option-rows',
    area: 'chat',
    title: 'Model selector option sub-menu',
    source: {
      overlay: [
        'components/chat/agent-model-selector/SelectorOptionRows.tsx',
        'components/chat/agent-model-selector/SelectorOptionRows.tsx'
      ]
    },
    blocked: NO_OPTIONS,
    steps: []
  },
  {
    id: 'chat.selector-effort-rows',
    area: 'chat',
    title: 'Model selector effort sub-menu',
    source: {
      overlay: [
        'components/chat/agent-model-selector/SelectorOptionRows.tsx',
        'components/chat/agent-model-selector/SelectorOptionRows.tsx'
      ]
    },
    blocked: NO_OPTIONS,
    steps: []
  },
  {
    id: 'chat.claude-account-choice',
    area: 'chat',
    title: 'Claude account choice menu at the limit',
    source: { overlay: ['components/chat/claude-account/AccountChoiceMenu.tsx'] },
    blocked: NO_CLAUDE,
    steps: []
  },
  {
    id: 'chat.claude-account-chip',
    area: 'chat',
    title: 'Claude account chip menu',
    source: { overlay: ['components/chat/claude-account/ClaudeAccountChip.tsx'] },
    blocked: NO_CLAUDE,
    steps: []
  },
  {
    id: 'chat.codex-account-chip',
    area: 'chat',
    title: 'Codex account chip menu',
    source: { overlay: ['components/chat/codex-account/CodexAccountChip.tsx'] },
    blocked: NO_CODEX,
    steps: []
  },
  {
    id: 'chat.codex-limit-notice',
    area: 'chat',
    title: 'Codex limit notice menu',
    source: { overlay: ['components/chat/codex-account/CodexLimitNotice.tsx'] },
    blocked: NO_CODEX,
    steps: []
  },
  {
    id: 'chat.launcher-overlay',
    area: 'chat',
    title: 'Agent launcher overlay',
    source: { overlay: ['components/workspace/PaneContent.tsx'] },
    blocked: LAUNCHER_FALLBACK,
    steps: []
  },
  {
    id: 'chat.launcher-agent-picker',
    area: 'chat',
    title: 'Launcher agent picker',
    source: { overlay: ['components/agents/launcher/pickers.tsx'] },
    blocked: LAUNCHER_FALLBACK,
    steps: []
  },
  {
    id: 'chat.launcher-model-picker',
    area: 'chat',
    title: 'Launcher model picker',
    source: { overlay: ['components/agents/launcher/pickers.tsx'] },
    blocked: LAUNCHER_FALLBACK,
    steps: []
  },
  {
    id: 'chat.browser-auth',
    area: 'chat',
    title: 'Agent browser sign-in dialog',
    source: { overlay: ['components/agents/BrowserAuthDialog.tsx'] },
    blocked: 'needs an agent that asks for a browser sign-in',
    steps: []
  },
  {
    id: 'chat.sign-in-elicitation',
    area: 'chat',
    title: 'Agent sign-in elicitation dialog',
    source: { overlay: ['components/agents/SignInElicitationDialog.tsx'] },
    blocked: 'needs an agent that asks for a sign-in',
    steps: []
  }
]
