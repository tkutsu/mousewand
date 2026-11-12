// Tiles the guest test's windows like sway does: foot left, "other" right.
// GNOME lets no client place itself, so this is the KWin script's twin.
import { Extension } from "resource:///org/gnome/shell/extensions/extension.js";

export default class Tile extends Extension {
    enable() {
        this._created = global.display.connect("window-created", (_, window) => {
            const actor = window.get_compositor_private();
            const id = actor.connect("first-frame", () => {
                actor.disconnect(id);
                const left = window.get_wm_class() === "foot";
                window.move_resize_frame(false, left ? 0 : 800, 0, 800, 900);
            });
        });
    }

    disable() {
        global.display.disconnect(this._created);
    }
}
