/**
 * The WebView's own context menu (Back, Reload, Inspect Element) makes no sense in an app;
 * Vitra shows native menus where it has something to offer. Text fields keep theirs for
 * copy and paste.
 */
export function suppressWebViewMenu(): void {
  document.addEventListener('contextmenu', (event) => {
    const target = event.target;
    const editable =
      target instanceof HTMLElement &&
      (target.isContentEditable || target.closest('input, textarea') !== null);
    if (!editable) {
      event.preventDefault();
    }
  });
}
