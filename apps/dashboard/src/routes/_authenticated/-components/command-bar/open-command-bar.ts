/** What the mounted `CommandBar` listens for to open. */
export const OPEN_EVENT = 'rustrak:command-bar';

/**
 * Opens the bar from outside the sidebar: the phone's tab bar, which has no
 * keyboard to press the shortcut with. One bar, mounted once with the sidebar,
 * so the shortcut is never bound twice.
 */
export const openCommandBar = () => window.dispatchEvent(new Event(OPEN_EVENT));
