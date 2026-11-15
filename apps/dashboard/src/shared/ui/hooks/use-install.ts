import { useSyncExternalStore } from 'react';

/**
 * Whether the dashboard can be installed from here, and how.
 *
 * - `installed`: already running as the installed app.
 * - `prompt`: Chromium handed over its install prompt; one tap installs.
 * - `manual`: iOS and iPadOS, where Safari has no prompt to hand over and
 *   installing is Share → Add to Home Screen, so the button explains that.
 * - `none`: nothing to offer (a desktop Firefox, or already dismissed by the
 *   browser).
 */
export type InstallMode = 'installed' | 'prompt' | 'manual' | 'none';

interface InstallPromptEvent extends Event {
  prompt: () => Promise<void>;
  userChoice: Promise<{ outcome: 'accepted' | 'dismissed' }>;
}

declare global {
  interface Window {
    /** Parked by the inline script in `index.html`, before the bundle ran. */
    __installPrompt?: InstallPromptEvent;
  }
  interface Navigator {
    /** iOS Safari's own flag for a home-screen web app. */
    standalone?: boolean;
  }
}

export const isStandalone = () =>
  window.matchMedia('(display-mode: standalone)').matches ||
  navigator.standalone === true;

const isIos = () =>
  /iPad|iPhone|iPod/.test(navigator.userAgent) ||
  // iPadOS asks for the desktop site and says it is a Mac.
  (navigator.userAgent.includes('Macintosh') && navigator.maxTouchPoints > 1);

function current(): InstallMode {
  if (isStandalone()) return 'installed';
  if (window.__installPrompt) return 'prompt';
  if (isIos()) return 'manual';
  return 'none';
}

function subscribe(notify: () => void) {
  const installed = () => {
    window.__installPrompt = undefined;
    notify();
  };
  window.addEventListener('rustrak:installable', notify);
  window.addEventListener('appinstalled', installed);
  return () => {
    window.removeEventListener('rustrak:installable', notify);
    window.removeEventListener('appinstalled', installed);
  };
}

export function useInstallMode(): InstallMode {
  return useSyncExternalStore(subscribe, current, () => 'none');
}

/**
 * Shows Chromium's install dialog. The event is single-use: whatever the
 * answer, it is spent, and the browser fires a fresh one if it is offered
 * again.
 */
export async function promptInstall(): Promise<boolean> {
  const event = window.__installPrompt;
  if (!event) return false;
  window.__installPrompt = undefined;
  await event.prompt();
  const { outcome } = await event.userChoice;
  window.dispatchEvent(new Event('rustrak:installable'));
  return outcome === 'accepted';
}
