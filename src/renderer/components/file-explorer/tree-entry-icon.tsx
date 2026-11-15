import { File, Folder } from '@/components/icons'
import { cn } from '@/lib/utils'

/**
 * File-type tint of the outline file icon (Paper 3.2 "File tree"). Files of
 * other types and all folders stay muted.
 */
const FILE_TONE_BY_EXTENSION: Record<string, string> = {
  css: 'text-project-purple',
  ts: 'text-project-blue',
  tsx: 'text-project-blue',
  md: 'text-connection',
  json: 'text-project-yellow'
}

/** Tint class of a tree file icon (exported for tests). */
export function treeFileIconTone(name: string): string {
  const dot = name.lastIndexOf('.')
  const extension = dot > 0 ? name.slice(dot + 1).toLowerCase() : ''
  return FILE_TONE_BY_EXTENSION[extension] ?? 'text-muted-foreground'
}

/** Outline folder or file icon of a file tree row (14px). */
export function TreeEntryIcon({
  name,
  isDirectory,
  className
}: {
  name: string
  isDirectory: boolean
  className?: string
}): React.JSX.Element {
  const Icon = isDirectory ? Folder : File
  return (
    <Icon
      size={14}
      data-testid="tree-entry-icon"
      className={cn(
        'shrink-0',
        isDirectory ? 'text-muted-foreground' : treeFileIconTone(name),
        className
      )}
    />
  )
}
