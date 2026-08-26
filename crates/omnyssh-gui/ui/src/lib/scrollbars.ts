const ACTIVE_CLASS = 'scrollbar-active';
const DEFAULT_IDLE_MS = 800;

/**
 * Keep native scrolling semantics while presenting an overlay-style thumb:
 * every scrollable element reveals its thumb during real scroll activity and
 * returns to the transparent resting state shortly after movement stops.
 */
export function installAutoHideScrollbars(
  root: Document = document,
  idleMs = DEFAULT_IDLE_MS
): () => void {
  const timers = new Map<Element, number>();
  const scheduler = root.defaultView ?? window;

  const onScroll = (event: Event): void => {
    const target =
      event.target === root
        ? root.scrollingElement
        : event.target instanceof Element
          ? event.target
          : null;
    if (!target) return;

    target.classList.add(ACTIVE_CLASS);
    const previous = timers.get(target);
    if (previous !== undefined) scheduler.clearTimeout(previous);
    const timer = scheduler.setTimeout(() => {
      target.classList.remove(ACTIVE_CLASS);
      timers.delete(target);
    }, idleMs);
    timers.set(target, timer);
  };

  root.addEventListener('scroll', onScroll, true);
  return () => {
    root.removeEventListener('scroll', onScroll, true);
    for (const [target, timer] of timers) {
      scheduler.clearTimeout(timer);
      target.classList.remove(ACTIVE_CLASS);
    }
    timers.clear();
  };
}
