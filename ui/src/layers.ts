// Overlay layers, front-most last (ADR-0044).
//
// Overlay precedence used to be a chain of early-returning if-statements maintained in three places
// per window — the window's key guard, `back()`, and (for one card) a window-level Esc check — so
// every new overlay cost three coordinated edits. The two windows had already drifted: the Side
// View's `back()` closed the model switcher, the panel's did not, so Esc with the switcher open and
// the focus elsewhere backed out of the page instead of closing the switcher. This is the one list.
//
// NOT every overlay belongs here. The stop-confirmation dialog, attachment mode and a streaming
// item's stop-confirm hold their own precedence ABOVE the cards — ADR-0044 makes that the refactor's
// hard constraint. Those are modal states of the surface, not cards: they never coexist with the
// cards, and their keys are handled before this module is consulted.

/** Anything that can be open over a surface and closed in one step. */
export interface Layer {
  /** A stable name. Nothing keys off it; it is for reading the stack and for future assertions. */
  name: string;
  isOpen(): boolean;
  close(): void;
}

export interface Layers {
  /** Add a layer. Registration order is front-most-LAST, which is also the order their anchors stack. */
  register(layer: Layer): void;
  /** The front-most open layer, or undefined when the surface has none open. */
  top(): Layer | undefined;
  /**
   * Is any layer open? The surface's "front-most list owns the keys" guard (ADR-0031): the card's own
   * input stops propagation for the keys it consumes, and this covers focus sitting elsewhere.
   */
  anyOpen(): boolean;
  /**
   * Close the front-most open layer. Returns false when none was open, so a back handler can fall
   * through to the next layer of the surface.
   */
  closeTop(): boolean;
}

export function createLayers(): Layers {
  const registered: Layer[] = [];

  const top = (): Layer | undefined => {
    for (let index = registered.length - 1; index >= 0; index -= 1) {
      if (registered[index].isOpen()) return registered[index];
    }
    return undefined;
  };

  return {
    register: (layer) => {
      registered.push(layer);
    },
    top,
    anyOpen: () => registered.some((layer) => layer.isOpen()),
    closeTop: () => {
      const layer = top();
      if (!layer) return false;
      layer.close();
      return true;
    },
  };
}