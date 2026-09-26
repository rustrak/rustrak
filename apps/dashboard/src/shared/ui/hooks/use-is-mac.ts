const IS_MAC = /mac|iphone|ipad|ipod/i.test(navigator.userAgent);

/** Whether the shortcut hints should read ⌘ rather than Ctrl. */
export function useIsMac(): boolean {
  return IS_MAC;
}
