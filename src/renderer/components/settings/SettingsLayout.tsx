import {
  createContext,
  Fragment,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState
} from 'react'
import { ArrowLeft, Search, X } from '@/components/icons'
import { TrafficLightClearance, WindowControls } from '@/components/shell/TopRowChrome'
import { useTopRowPlatform } from '@/components/shell/top-row-context'
import {
  FOCUS_RING_CLASS,
  PANEL_FIELD_ICON_CLASS,
  QUIET_ICON_BUTTON_CLASS
} from '@/components/ui/panel-styles'
import { type SettingsSearchEntry, searchSettings } from '@/lib/settings-search'
import { cn } from '@/lib/utils'

/** A settings page, shown in the rail. */
export interface SettingsCategory {
  /** Stable id; also the page id (`data-settings-section`). */
  id: string
  /** Rail label and page title. */
  label: string
  /** One line of help under the page title. */
  description?: string
  /** Rail group heading. Consecutive pages with the same group share it. */
  group?: string
  /** Optional icon rendered before the label. */
  icon?: React.ReactNode
}

interface SettingsLayoutProps {
  /** Pages in display order. */
  categories: SettingsCategory[]
  /** Flat search index across every page. */
  searchIndex: readonly SettingsSearchEntry[]
  /** Pages, each wrapped in {@link SettingsSection}. Every page stays mounted. */
  children: React.ReactNode
  /** Name of the settings, e.g. "Preferences". Names the narrow Close button. */
  title: string
  /**
   * Optional line under the title, e.g. the project name. When set, the rail
   * shows the title and this line under Back to app.
   */
  subtitle?: React.ReactNode
  /** Leave the settings: the rail's "Back to app" and the narrow Close button. */
  onClose?: () => void
  /** Controlled active page id. */
  page?: string
  /** Initial page when uncontrolled. Defaults to the first category. */
  defaultPage?: string
  /** Called when the active page changes. */
  onPageChange?: (page: string) => void
  /** Optional extra content at the bottom of the rail. */
  sidebarFooter?: React.ReactNode
  /** Optional bar pinned under the page, e.g. the Project settings Save bar. */
  footer?: React.ReactNode
}

/** How long a search jump marks its target row. */
export const SETTINGS_TARGET_MS = 2000

/**
 * Sent (bubbling) from a search target that sits inside a closed part of the
 * page, such as an Advanced card, so that part opens before the jump.
 */
export const SETTINGS_REVEAL_EVENT = 'settings:reveal'

/** Width of the page column on every page, so nothing jumps sideways between pages. */
export const SETTINGS_COLUMN_CLASS = 'mx-auto w-full max-w-[640px]'

const ActivePageContext = createContext<string | undefined>(undefined)
const SectionVisibleContext = createContext(true)

/** False while this settings page is mounted but hidden behind another page. */
export function useSettingsSectionVisible(): boolean {
  return useContext(SectionVisibleContext)
}

/** A 28px rail row: page, search result or Back to app. */
const RAIL_ROW_CLASS = `flex flex-shrink-0 items-center gap-2 rounded-md px-2 text-left text-xs font-medium transition-colors duration-150 ease-out md:w-full pointer-coarse:min-h-11 active:bg-foreground/[0.06] ${FOCUS_RING_CLASS}`
const RAIL_IDLE_CLASS = 'text-secondary-foreground hover:bg-foreground/[0.04] hover:text-foreground'
const RAIL_ACTIVE_CLASS = 'bg-foreground/10 text-foreground'

/**
 * One settings page. It stays mounted (form state and search anchors keep
 * working) and is hidden unless it is the active page. Sections on a page
 * are 32px apart.
 */
export function SettingsSection({
  id,
  children,
  className
}: {
  id: string
  children: React.ReactNode
  className?: string
}): React.JSX.Element {
  const activeId = useContext(ActivePageContext)
  const hidden = activeId !== undefined && activeId !== id
  return (
    <section
      id={`settings-section-${id}`}
      data-settings-section={id}
      hidden={hidden}
      className={cn('flex flex-col gap-8', className)}
    >
      <SectionVisibleContext.Provider value={!hidden}>{children}</SectionVisibleContext.Provider>
    </section>
  )
}

/**
 * Shared full-window settings shell: a rail with "Back to app", search and
 * grouped pages, and one page at a time in a 640px column. Search results open
 * the page, scroll to the setting and mark it for {@link SETTINGS_TARGET_MS}.
 * Under the `md` breakpoint the rail becomes a top bar with a scrolling strip
 * and a Close button.
 *
 * On the desktop the rail's top row is a window drag handle with room for the
 * macOS traffic lights, and the page's top row holds the Windows and Linux
 * window controls.
 */
export function SettingsLayout({
  categories,
  searchIndex,
  children,
  title,
  subtitle,
  onClose,
  page,
  defaultPage,
  onPageChange,
  sidebarFooter,
  footer
}: SettingsLayoutProps): React.JSX.Element {
  const platform = useTopRowPlatform()
  const drag = platform.dragRegions ? true : undefined
  const bodyRef = useRef<HTMLDivElement | null>(null)
  const [uncontrolledPage, setUncontrolledPage] = useState(defaultPage ?? categories[0]?.id)
  const activeId = page ?? uncontrolledPage
  const [query, setQuery] = useState('')
  const targetTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const markedRef = useRef<HTMLElement | null>(null)
  const rafRef = useRef<number | null>(null)

  const results = useMemo(() => searchSettings(query, searchIndex), [query, searchIndex])
  const isSearching = query.trim().length > 0
  const active = categories.find((category) => category.id === activeId) ?? categories[0]

  const selectPage = useCallback(
    (id: string) => {
      if (page === undefined) setUncontrolledPage(id)
      onPageChange?.(id)
    },
    [page, onPageChange]
  )

  const clearMark = useCallback(() => {
    if (targetTimerRef.current) clearTimeout(targetTimerRef.current)
    targetTimerRef.current = null
    markedRef.current?.removeAttribute('data-settings-target')
    markedRef.current = null
  }, [])

  const cancelJump = useCallback(() => {
    if (rafRef.current === null) return
    cancelAnimationFrame(rafRef.current)
    rafRef.current = null
  }, [])

  useEffect(() => {
    return () => {
      cancelJump()
      clearMark()
    }
  }, [cancelJump, clearMark])

  // A new page starts at the top.
  useEffect(() => {
    if (activeId === undefined) return
    bodyRef.current?.scrollTo({ top: 0 })
  }, [activeId])

  const openResult = useCallback(
    (categoryId: string, anchorId?: string) => {
      selectPage(categoryId)
      clearMark()
      cancelJump()
      if (!anchorId) return
      // Wait one frame so the page is visible before measuring it.
      rafRef.current = requestAnimationFrame(() => {
        rafRef.current = null
        clearMark()
        const target = bodyRef.current?.querySelector<HTMLElement>(`#${CSS.escape(anchorId)}`)
        if (!target) return
        const mark = (): void => {
          target.scrollIntoView({ behavior: 'smooth', block: 'start' })
          target.setAttribute('data-settings-target', '')
          markedRef.current = target
          targetTimerRef.current = setTimeout(clearMark, SETTINGS_TARGET_MS)
        }
        // A row inside a closed Advanced card: ask the card to open, then
        // wait one more frame so the row is laid out before scrolling to it.
        if (target.closest('[hidden]')) {
          target.dispatchEvent(new CustomEvent(SETTINGS_REVEAL_EVENT, { bubbles: true }))
          rafRef.current = requestAnimationFrame(() => {
            rafRef.current = null
            mark()
          })
          return
        }
        mark()
      })
    },
    [selectPage, clearMark, cancelJump]
  )

  const handleCategoryKeyDown = (
    event: React.KeyboardEvent<HTMLButtonElement>,
    index: number
  ): void => {
    const forward = event.key === 'ArrowDown' || event.key === 'ArrowRight'
    const back = event.key === 'ArrowUp' || event.key === 'ArrowLeft'
    if (!forward && !back) return
    event.preventDefault()
    const next = (index + (forward ? 1 : -1) + categories.length) % categories.length
    const nav = event.currentTarget.closest('nav')
    nav?.querySelectorAll<HTMLButtonElement>('[data-category-button]')[next]?.focus()
  }

  return (
    <ActivePageContext.Provider value={active?.id}>
      <div className="flex min-h-0 flex-1 flex-col overflow-hidden md:flex-row">
        <aside className="flex flex-shrink-0 flex-col border-b border-border bg-sidebar md:w-60 md:border-b-0">
          {/* Narrow: a title bar with Close. Desktop: the window's drag row. */}
          <div className="flex min-h-12 items-center gap-2 pl-4 pr-2.5 pt-[env(safe-area-inset-top)] md:hidden">
            <div className="min-w-0 flex-1 py-2">
              <h2 className="truncate text-sm font-semibold text-foreground">{title}</h2>
              {subtitle && <p className="truncate text-xs text-muted-foreground">{subtitle}</p>}
            </div>
            {onClose && (
              <button
                type="button"
                onClick={onClose}
                aria-label={`Close ${title}`}
                title="Close"
                className={cn(QUIET_ICON_BUTTON_CLASS, 'size-7 shrink-0 pointer-coarse:size-11')}
              >
                <X size={16} />
              </button>
            )}
          </div>
          <div className="hidden h-11 shrink-0 md:flex" data-tauri-drag-region={drag}>
            <TrafficLightClearance trafficLights={platform.trafficLights} />
          </div>

          <div className="hidden flex-col gap-1 px-3 md:flex">
            {onClose && (
              <button
                type="button"
                onClick={onClose}
                className={cn(RAIL_ROW_CLASS, 'h-7', RAIL_IDLE_CLASS)}
              >
                <ArrowLeft size={14} aria-hidden />
                <span className="truncate">Back to app</span>
              </button>
            )}
            {subtitle && (
              <div className="min-w-0 px-2 pb-1 pt-2">
                <p className="truncate text-xs font-medium text-foreground">{title}</p>
                <p className="truncate text-xs text-muted-foreground">{subtitle}</p>
              </div>
            )}
          </div>

          <div className="px-3 pb-2 md:pt-1">
            <div className="relative">
              <Search size={13} aria-hidden className={PANEL_FIELD_ICON_CLASS} />
              <input
                type="text"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder="Search settings…"
                aria-label="Search settings"
                autoCapitalize="off"
                autoCorrect="off"
                spellCheck={false}
                enterKeyHint="search"
                className={cn(
                  'h-7 w-full rounded-md border-0 bg-foreground/[0.04] pl-7 pr-7 text-xs font-medium text-foreground placeholder:text-muted-foreground',
                  'pointer-coarse:h-11 pointer-coarse:pr-11 pointer-coarse:text-base',
                  FOCUS_RING_CLASS
                )}
              />
              {query && (
                <button
                  type="button"
                  onClick={() => setQuery('')}
                  aria-label="Clear search"
                  className={cn(
                    QUIET_ICON_BUTTON_CLASS,
                    'absolute right-0.5 top-1/2 flex size-6 -translate-y-1/2 pointer-coarse:right-0 pointer-coarse:size-11'
                  )}
                >
                  <X size={13} />
                </button>
              )}
            </div>
          </div>

          <nav
            aria-label="Settings pages"
            className="flex flex-1 gap-px overflow-x-auto px-3 pb-2 md:flex-col md:overflow-y-auto md:pb-3"
          >
            {isSearching ? (
              results.length === 0 ? (
                <div className="flex flex-col items-start gap-2 px-2 py-3">
                  <p className="text-xs text-muted-foreground">
                    No settings match "{query.trim()}".
                  </p>
                  <button
                    type="button"
                    onClick={() => setQuery('')}
                    className={cn(
                      'h-7 rounded-md bg-muted px-3 text-xs font-medium text-foreground shadow-control hover:bg-secondary pointer-coarse:h-11',
                      FOCUS_RING_CLASS
                    )}
                  >
                    Clear search
                  </button>
                </div>
              ) : (
                <>
                  <p className="hidden px-2 pb-1 pt-2 text-xs text-muted-foreground md:block">
                    {results.length === 1 ? '1 result' : `${results.length} results`}
                  </p>
                  {results.map((result) => {
                    const category = categories.find((c) => c.id === result.categoryId)
                    return (
                      <button
                        key={`${result.categoryId}-${result.label}`}
                        type="button"
                        onClick={() => openResult(result.categoryId, result.anchorId)}
                        className={cn(RAIL_ROW_CLASS, 'min-h-8 py-1.5', RAIL_IDLE_CLASS)}
                      >
                        <span className="flex min-w-0 flex-col items-start gap-0.5">
                          <span className="max-w-full truncate">{result.label}</span>
                          <span className="max-w-full truncate font-normal text-muted-foreground">
                            {category?.group ? `${category.group} · ` : ''}
                            {category?.label}
                          </span>
                        </span>
                      </button>
                    )
                  })}
                </>
              )
            ) : (
              categories.map((category, index) => {
                const isActive = category.id === active?.id
                const startsGroup =
                  category.group !== undefined && category.group !== categories[index - 1]?.group
                return (
                  <Fragment key={category.id}>
                    {startsGroup && (
                      <p
                        data-testid="settings-group-label"
                        className={cn(
                          'hidden px-2 pb-1 text-xs text-muted-foreground md:block',
                          index === 0 ? 'pt-1' : 'pt-4'
                        )}
                      >
                        {category.group}
                      </p>
                    )}
                    <button
                      type="button"
                      data-category-button
                      aria-current={isActive ? 'page' : undefined}
                      onClick={() => selectPage(category.id)}
                      onKeyDown={(e) => handleCategoryKeyDown(e, index)}
                      className={cn(
                        RAIL_ROW_CLASS,
                        'h-7 [&_svg]:size-3.5',
                        isActive ? RAIL_ACTIVE_CLASS : RAIL_IDLE_CLASS
                      )}
                    >
                      {category.icon && (
                        <span className="flex flex-shrink-0 items-center">{category.icon}</span>
                      )}
                      <span className="truncate">{category.label}</span>
                    </button>
                  </Fragment>
                )
              })
            )}
          </nav>

          {sidebarFooter && <div className="border-t border-border p-2">{sidebarFooter}</div>}
        </aside>

        <div className="flex min-h-0 min-w-0 flex-1 flex-col">
          {platform.dragRegions && (
            <div
              className="hidden h-11 shrink-0 items-center justify-end md:flex"
              data-tauri-drag-region={drag}
            >
              {platform.windowControls && <WindowControls windowControls />}
            </div>
          )}
          <div
            ref={bodyRef}
            className="min-h-0 flex-1 overflow-y-auto overscroll-contain px-4 pb-20 pt-6 md:px-8 md:pt-4"
          >
            <div className={SETTINGS_COLUMN_CLASS}>
              {active && (
                <header className="mb-8 flex flex-col gap-1.5">
                  <h2 className="text-xl font-medium tracking-[-0.01em] text-foreground">
                    {active.label}
                  </h2>
                  {active.description && (
                    <p className="text-sm text-muted-foreground">{active.description}</p>
                  )}
                </header>
              )}
              {children}
            </div>
          </div>
          {footer && (
            <div className="shrink-0 border-t border-border bg-background px-4 py-3 pb-[max(0.75rem,env(safe-area-inset-bottom))] md:px-8">
              <div className={SETTINGS_COLUMN_CLASS}>{footer}</div>
            </div>
          )}
        </div>
      </div>
    </ActivePageContext.Provider>
  )
}
