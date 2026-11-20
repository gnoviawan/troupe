import { useMemo, useState } from 'react'
import { SheetGroup, SheetRow, SheetSearchField } from '@/components/ui/mobile-sheet'
import { ListMessage, TabGlyph } from '../SelectorRows'
import {
  type AgentGroup,
  type AgentTab,
  agentTabSelected,
  groupAgentTabs,
  isInstalledTab
} from '../selector-model'
import { useKnownCatalogs } from '../use-model-catalog'
import { setupRouteKey } from './MobileProviderSetup'

/** The sheet route key of an agent's install view, as its row's `subview`. */
export const installRouteKey = (configId: string): string => `install:${configId}`

export const PROVIDERS_ROUTE_KEY = 'providers'

const modelCount = (count: number): string => `${count} ${count === 1 ? 'model' : 'models'}`

/**
 * Every agent, grouped by what a tap does: Ready shows its models, Needs setup
 * opens its setup screen (the chat cannot arm it until its config is saved),
 * Install opens its install view. The groups are the selector's own
 * (`agentGroup`), shared with the desktop "More" list.
 */
export function ProviderList({
  tabs,
  viewId,
  cwd,
  ownCatalog,
  disabled,
  badgeOf,
  groupOf,
  onPick
}: {
  tabs: readonly AgentTab[]
  viewId: string | null
  cwd: string
  /** The composer agent's model count, which the known lists may not hold. */
  ownCatalog: { configId: string | null; count: number | null }
  disabled: boolean
  badgeOf: (tab: AgentTab) => string | null
  groupOf: (tab: AgentTab) => AgentGroup
  onPick: (tab: AgentTab) => void
}): React.JSX.Element {
  const [query, setQuery] = useState('')
  const readyIds = useMemo(
    () => tabs.filter((tab) => isInstalledTab(tab)).map((tab) => tab.configId),
    [tabs]
  )
  const known = useKnownCatalogs(cwd, readyIds)
  const countOf = (configId: string): number | null => {
    if (configId === ownCatalog.configId && ownCatalog.count !== null) return ownCatalog.count
    return known[readyIds.indexOf(configId)]?.options.length ?? null
  }

  const needle = query.trim().toLowerCase()
  const shown = needle ? tabs.filter((tab) => tab.name.toLowerCase().includes(needle)) : tabs
  return (
    <>
      <SheetSearchField
        value={query}
        onChange={setQuery}
        placeholder="Search providers…"
        label="Search providers"
      />
      {shown.length === 0 ? (
        <ListMessage title="No providers match." detail="Try another name." />
      ) : null}
      {groupAgentTabs(shown, groupOf).map(({ group, label, tabs: rows }) => {
        return (
          <SheetGroup key={group} label={label}>
            {rows.map((tab) => {
              const icon = <TabGlyph tab={tab} />
              const badge = badgeOf(tab) ?? undefined
              if (group === 'ready') {
                const count = countOf(tab.configId)
                return (
                  <SheetRow
                    key={tab.configId}
                    icon={icon}
                    title={tab.name}
                    subtitle={count === null ? undefined : modelCount(count)}
                    value={badge}
                    accessory="check"
                    selected={agentTabSelected(tab, viewId)}
                    disabled={disabled}
                    onSelect={() => onPick(tab)}
                  />
                )
              }
              if (group === 'setup') {
                return (
                  <SheetRow
                    key={tab.configId}
                    icon={icon}
                    title={tab.name}
                    value={badge}
                    accessory="chevron"
                    subview={setupRouteKey(tab.configId)}
                    disabled={disabled}
                    onSelect={() => onPick(tab)}
                  />
                )
              }
              return (
                <SheetRow
                  key={tab.configId}
                  icon={icon}
                  title={tab.name}
                  value={badge}
                  accessory="chevron"
                  subview={installRouteKey(tab.configId)}
                  disabled={disabled}
                  onSelect={() => onPick(tab)}
                />
              )
            })}
          </SheetGroup>
        )
      })}
    </>
  )
}
