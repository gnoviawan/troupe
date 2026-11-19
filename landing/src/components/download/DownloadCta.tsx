import { Button, type ButtonSize } from '@/components/ui/Button';
import { AppleIcon, ChevronIcon, OsIcon } from '@/components/ui/Icons';
import { cn } from '@/lib/utils';
import { useDownload } from './DownloadContext';
import * as DownloadMenu from './DownloadMenu';
import * as DownloadSheet from './DownloadSheet';

/**
 * The page's main download control:
 * - from 768px: "Download for <OS>" opens the Stable / Nightly menu;
 * - below 768px: "Get Troupe for your Mac" opens the sheet (a phone cannot run the app).
 * Both render; CSS shows one, so prerendered HTML and hydration match.
 */
export function DownloadCta({
  size = 'lg',
  className,
  compact = false,
}: {
  size?: ButtonSize;
  className?: string;
  /** Nav version: "Download" only, small. */
  compact?: boolean;
}) {
  const { platform, osName } = useDownload();

  return (
    <>
      <div className={cn('hidden md:flex', className)}>
        <DownloadMenu.Root>
          <DownloadMenu.Trigger
            aria-label={compact ? `Download for ${osName}` : undefined}
            render={<Button size={compact ? 'sm' : size} className={compact ? 'pr-3.5 pl-3' : undefined} />}
          >
            <OsIcon os={platform.os} size={compact ? 14 : 16} className="-mt-0.5" />
            {compact ? 'Download' : `Download for ${osName}`}
            {compact ? null : (
              <ChevronIcon
                size={14}
                className="-mr-3 -ml-0.5 opacity-85 transition-transform duration-150 ease-(--ease-out) in-aria-expanded:rotate-180"
              />
            )}
          </DownloadMenu.Trigger>
          <DownloadMenu.Content>
            <DownloadMenu.Group>
              <DownloadMenu.Label />
              <DownloadMenu.Channel value="stable" />
              <DownloadMenu.Channel value="nightly" />
            </DownloadMenu.Group>
            <DownloadMenu.Footer />
          </DownloadMenu.Content>
        </DownloadMenu.Root>
      </div>

      <div className={cn('flex w-full md:hidden', compact && 'w-auto', className)}>
        <DownloadSheet.Root>
          <DownloadSheet.Trigger render={<Button size={compact ? 'sm' : 'touch'} />}>
            {compact ? null : <AppleIcon size={17} className="-mt-0.5" />}
            {compact ? 'Get it' : 'Get Troupe for your Mac'}
          </DownloadSheet.Trigger>
          <DownloadSheet.Content>
            <DownloadSheet.Header />
            <DownloadSheet.ChannelPicker />
            <DownloadSheet.Actions />
            <DownloadSheet.Footer />
          </DownloadSheet.Content>
        </DownloadSheet.Root>
      </div>
    </>
  );
}
