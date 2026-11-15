import { useCallback } from 'react'
import { useLocation, useNavigate } from 'react-router-dom'
import { openNewChat } from '@/components/chat/new-chat/open-new-chat'

/**
 * The Rail's New chat: a new empty chat in the active pane (`openNewChat`).
 * Chat tabs only render on the workspace route, so a child route goes back
 * to `/` first and the chat opens on the next frame.
 */
export function useNewChat(): () => void {
  const pathname = useLocation().pathname
  const navigate = useNavigate()

  return useCallback(() => {
    const open = (): void => void openNewChat({ reason: 'rail' })
    if (pathname !== '/') {
      navigate('/')
      requestAnimationFrame(open)
    } else {
      open()
    }
  }, [navigate, pathname])
}
