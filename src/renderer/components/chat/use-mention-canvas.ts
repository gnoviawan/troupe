import { useEffect, useRef, useState } from 'react'
import {
  type CanvasLayerMatch,
  layerSize,
  readCanvasMentionTargets,
  selectionLabel
} from '@/lib/canvas-mention'
import { logFrontendError } from '@/lib/log-api'
import { useCanvasStore } from '@/stores/canvas-store'
import type { CanvasListing } from './mention-menu-model'

function layerDetail(layer: CanvasLayerMatch): string {
  const size = layerSize(layer)
  return size ? `${layer.type} · ${size}` : layer.type
}

/**
 * The @ menu's canvas data. `available` is true while the project's canvas
 * is open and ready; `listing` is read each time the Canvas panel opens
 * (`open`), so the selection is the one the user has when they look.
 */
export function useMentionCanvas(
  projectId: string | null | undefined,
  open: boolean
): { available: boolean; listing: CanvasListing | undefined } {
  const available = useCanvasStore((state) =>
    projectId ? state.sessions[projectId]?.phase.kind === 'ready' : false
  )
  const [listing, setListing] = useState<CanvasListing | undefined>(undefined)
  const genRef = useRef(0)

  useEffect(() => {
    genRef.current += 1
    if (!open || !projectId || !available) {
      setListing(undefined)
      return
    }
    const gen = genRef.current
    setListing({ status: 'loading' })
    readCanvasMentionTargets(projectId)
      .then((targets) => {
        if (gen !== genRef.current) return
        const { selection } = targets
        setListing({
          status: 'ready',
          pageName: targets.pageName,
          selection:
            selection.length === 0
              ? null
              : {
                  ref: { label: selectionLabel(selection), ids: selection.map((l) => l.id) },
                  detail:
                    selection.length === 1
                      ? layerDetail(selection[0])
                      : `${selection.length} selected layers`
                },
          layers: targets.layers.map((layer) => ({
            id: layer.id,
            name: layer.name || layer.type.toLowerCase(),
            detail: layerDetail(layer)
          }))
        })
      })
      .catch((err: unknown) => {
        if (gen !== genRef.current) return
        setListing({ status: 'error' })
        void logFrontendError({
          level: 'warn',
          source: 'useMentionCanvas',
          message: `composer mention canvas read failed: ${err instanceof Error ? err.message : String(err)}`
        })
      })
  }, [open, projectId, available])

  return { available, listing }
}
