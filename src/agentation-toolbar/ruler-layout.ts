export type GuideAxis = 'horizontal' | 'vertical'

export interface GuideLayoutInput {
  id: string
  axis: GuideAxis
  pos: number
  deltaPx?: number
}

export interface GuideBadgePlacement {
  id: string
  axis: GuideAxis
  text: string
  left: number
  top?: number
  bottom?: number
}

const BADGE_CHAR_PX = 7.2
const BADGE_PADDING_PX = 18
const BADGE_MIN_WIDTH_PX = 68
const BADGE_LANE_GAP_PX = 6
const VERTICAL_BASE_BOTTOM_PX = 14
const VERTICAL_LANE_STEP_PX = 24
const HORIZONTAL_BASE_LEFT_PX = 16
const HORIZONTAL_LANE_STEP_PX = 116
const HORIZONTAL_MIN_Y_GAP_PX = 24

export function formatGuideBadgeText(
  guide: Pick<GuideLayoutInput, 'axis' | 'pos' | 'deltaPx'>
): string {
  const prefix = guide.axis === 'vertical' ? `x ${guide.pos}px` : `y ${guide.pos}px`
  if (typeof guide.deltaPx === 'number' && guide.deltaPx !== 0) {
    return `${prefix} · ${guide.deltaPx > 0 ? `+${guide.deltaPx}` : guide.deltaPx}px off`
  }
  return prefix
}

function estimateBadgeWidth(text: string): number {
  return Math.max(BADGE_MIN_WIDTH_PX, Math.ceil(text.length * BADGE_CHAR_PX + BADGE_PADDING_PX))
}

/**
 * Compute non-overlapping badge placements for vertical and horizontal ruler guides.
 * Adjacent guides whose labels would collide are staggered into stacked lanes.
 */
export function computeGuideBadgeLayout(
  guides: ReadonlyArray<GuideLayoutInput>,
  viewportWidth: number,
  viewportHeight: number
): Map<string, GuideBadgePlacement> {
  const placements = new Map<string, GuideBadgePlacement>()
  const vw = Math.max(160, viewportWidth)
  const vh = Math.max(120, viewportHeight)

  // Vertical guides: badges live along the bottom edge and stack upward when close horizontally.
  const verticals = guides.filter((g) => g.axis === 'vertical').sort((a, b) => a.pos - b.pos)
  const laneRightEdges: number[] = []
  for (const guide of verticals) {
    const text = formatGuideBadgeText(guide)
    const width = estimateBadgeWidth(text)
    const left = Math.max(8, Math.min(vw - width - 8, guide.pos + 8))
    let lane = laneRightEdges.findIndex((edge) => edge + BADGE_LANE_GAP_PX <= left)
    if (lane === -1) {
      lane = laneRightEdges.length
      laneRightEdges.push(left + width)
    } else {
      laneRightEdges[lane] = left + width
    }
    placements.set(guide.id, {
      id: guide.id,
      axis: 'vertical',
      text,
      left,
      bottom: VERTICAL_BASE_BOTTOM_PX + lane * VERTICAL_LANE_STEP_PX
    })
  }

  // Horizontal guides: badges live along the left edge and shift right when close vertically.
  const horizontals = guides.filter((g) => g.axis === 'horizontal').sort((a, b) => a.pos - b.pos)
  const laneBottomEdges: number[] = []
  for (const guide of horizontals) {
    const text = formatGuideBadgeText(guide)
    const width = estimateBadgeWidth(text)
    const top = Math.max(8, Math.min(vh - 28, guide.pos + 6))
    let lane = laneBottomEdges.findIndex((edge) => edge + HORIZONTAL_MIN_Y_GAP_PX <= top)
    if (lane === -1) {
      lane = laneBottomEdges.length
      laneBottomEdges.push(top)
    } else {
      laneBottomEdges[lane] = top
    }
    const maxLeft = Math.max(8, vw - width - 8)
    const left = Math.min(maxLeft, HORIZONTAL_BASE_LEFT_PX + lane * HORIZONTAL_LANE_STEP_PX)
    placements.set(guide.id, {
      id: guide.id,
      axis: 'horizontal',
      text,
      left,
      top
    })
  }

  return placements
}

/**
 * Find the absolute pixel distance from `pos` to the nearest placed guide on the same axis.
 */
export function findNearestGuideGap(
  guides: ReadonlyArray<Pick<GuideLayoutInput, 'id' | 'axis' | 'pos'>>,
  axis: GuideAxis,
  pos: number,
  excludeId?: string
): number | null {
  let best: number | null = null
  for (const g of guides) {
    if (g.axis !== axis || g.id === excludeId) continue
    const gap = Math.abs(Math.round(pos - g.pos))
    if (best === null || gap < best) {
      best = gap
    }
  }
  return best
}
