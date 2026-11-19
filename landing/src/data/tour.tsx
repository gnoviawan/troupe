import { accountsTab } from '@/components/tour/tabs/accounts';
import { automationTab } from '@/components/tour/tabs/automation';
import { browserTab } from '@/components/tour/tabs/browser';
import { canvasTab } from '@/components/tour/tabs/canvas';
import { remoteTab } from '@/components/tour/tabs/remote';
import type { TourTab } from '@/components/tour/types';

/** Tour tab order (plan Q8/Q9). */
export const TOUR_TABS: TourTab[] = [accountsTab, canvasTab, automationTab, browserTab, remoteTab];
