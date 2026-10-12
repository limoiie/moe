// The overlay layer stack (ADR-0044).
//
// This module exists because overlay precedence used to be three hand-maintained if-chains per
// window, and the two windows had already drifted: the panel's back() never closed the model
// switcher while its key guard refused to let empty Backspace through, and the Side View's back()
// closed it. Nothing failed loudly — the bug was a missing branch.
//
// So these tests pin the two properties that drift silently: WHICH layer is front-most, and that a
// pop takes exactly one layer.

import { describe, expect, it } from "vitest";
import { createLayers, type Layer } from "./layers";

/** A layer the test drives: `show()` opens it, and `closes` counts how many times the stack closed it. */
function drivable(name: string) {
  let open = false;
  let closes = 0;
  const layer: Layer = {
    name,
    isOpen: () => open,
    close: () => {
      closes += 1;
      open = false;
    },
  };
  return {
    layer,
    name,
    show: () => {
      open = true;
    },
    isOpen: () => open,
    closes: () => closes,
  };
}

describe("createLayers", () => {
  it("reports nothing open on an empty stack, and pops nothing", () => {
    const layers = createLayers();
    expect(layers.anyOpen()).toBe(false);
    expect(layers.top()).toBeUndefined();
    expect(layers.closeTop()).toBe(false);
  });

  it("treats the last registered layer as front-most", () => {
    const actions = drivable("actions");
    const about = drivable("about");
    const switcher = drivable("model switcher");
    const layers = createLayers();
    // The panel's registration order, ADR-0044: front-most LAST.
    layers.register(actions.layer);
    layers.register(about.layer);
    layers.register(switcher.layer);

    actions.show();
    about.show();
    switcher.show();

    // Front-most wins regardless of the order they were opened in.
    expect(layers.top()?.name).toBe("model switcher");
  });

  it("skips closed layers when finding the front-most", () => {
    const actions = drivable("actions");
    const about = drivable("about");
    const layers = createLayers();
    layers.register(actions.layer);
    layers.register(about.layer);

    actions.show();
    // `about` — registered last, so front-most — is closed, so `actions` is what the surface sees.
    expect(layers.top()?.name).toBe("actions");
    expect(layers.anyOpen()).toBe(true);
  });

  it("pops exactly one layer per closeTop, front to back", () => {
    const actions = drivable("actions");
    const about = drivable("about");
    const switcher = drivable("model switcher");
    const layers = createLayers();
    layers.register(actions.layer);
    layers.register(about.layer);
    layers.register(switcher.layer);
    actions.show();
    about.show();
    switcher.show();

    expect(layers.closeTop()).toBe(true);
    expect(switcher.closes()).toBe(1);
    expect(switcher.isOpen()).toBe(false);
    // Only the front-most went: the layers behind it are untouched, which is what makes Back layered.
    expect(actions.isOpen()).toBe(true);
    expect(about.isOpen()).toBe(true);
    expect(about.closes()).toBe(0);

    expect(layers.closeTop()).toBe(true);
    expect(about.closes()).toBe(1);
    expect(actions.closes()).toBe(0);

    expect(layers.closeTop()).toBe(true);
    expect(actions.closes()).toBe(1);

    // Now empty: the caller can fall through to the next layer of the surface.
    expect(layers.closeTop()).toBe(false);
    expect(layers.anyOpen()).toBe(false);
  });

  it("re-reads isOpen rather than caching it", () => {
    const actions = drivable("actions");
    const layers = createLayers();
    layers.register(actions.layer);

    expect(layers.anyOpen()).toBe(false);
    actions.show();
    expect(layers.anyOpen()).toBe(true);
    // Something other than the stack closed it (e.g. the card's own Escape handling): the stack must
    // agree, since it never saw the change.
    layers.closeTop();
    expect(layers.anyOpen()).toBe(false);
    expect(layers.top()).toBeUndefined();
  });

  it("anyOpen is true when only a layer behind the front-most is open", () => {
    const actions = drivable("actions");
    const about = drivable("about");
    const layers = createLayers();
    layers.register(actions.layer);
    layers.register(about.layer);

    actions.show();
    about.show();
    about.layer.close(); // the front-most closes itself, one layer remains

    expect(layers.anyOpen()).toBe(true);
    expect(layers.top()?.name).toBe("actions");
  });
});