// The hint panel, drawn by GNOME Shell: the stroke so far as arrows, what
// it could still become, and what fired. It follows mousewand's own hint,
// with St labels instead of a painted image, and fades with the trail.

import Clutter from 'gi://Clutter';
import St from 'gi://St';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';

const ARROWS = {U: '↑', D: '↓', L: '←', R: '→', 7: '↖', 9: '↗', 1: '↙', 3: '↘'};

const DEFAULT_STYLE = {
    enabled: false,
    vertical: 'end',
    horizontal: 'center',
    margin: 64,
    font: 'sans-serif',
    size: 17,
    text: [0.95, 0.95, 0.97, 1],
    glyph: [0.62, 0.77, 1, 1],
    background: [0.11, 0.12, 0.15, 1],
    opacity: 0.9,
    padding: 14,
    radius: 14,
};

const css = ([r, g, b, a], alpha = 1) =>
    `rgba(${Math.round(r * 255)}, ${Math.round(g * 255)}, ${Math.round(b * 255)}, ${a * alpha})`;
const arrows = stroke => [...stroke].map(c => ARROWS[c] ?? '').join(' ');

export class Hint {
    constructor() {
        this._style = DEFAULT_STYLE;
        this._box = null;
    }

    setStyle(style) {
        this._style = {...DEFAULT_STYLE, ...style};
    }

    // `hint`: {stroke, label, state: 'drawing' | 'fired' | 'missed', next: [[rest, label]]}.
    show(hint, rejected) {
        const s = this._style;
        if (!s.enabled)
            return;
        this.destroy();
        const [arrow, text, label] = {
            drawing: [css(s.glyph), css(s.text, 0.75), hint.label],
            fired: [css(s.glyph), css(s.text), hint.label],
            missed: [css([...rejected, 1]), css(s.text, 0.6), 'No gesture'],
        }[hint.state];
        const rows = [];
        if (hint.stroke || label)
            rows.push({arrows: arrows(hint.stroke), label, size: s.size, arrow, text});
        const next = hint.next.map(([rest, l]) => ({
            arrows: arrows(rest), label: l, size: s.size * 0.82,
            arrow: css(s.glyph, 0.85), text: css(s.text, 0.7), next: true,
        }));
        if (rows.length + next.length === 0)
            return;
        // Next moves line their labels up after the longest stroke.
        const column = Math.max(0, ...next.map(r => [...r.arrows].filter(c => c !== ' ').length)) * s.size * 0.82 * 1.05;

        this._box = new St.BoxLayout({
            orientation: Clutter.Orientation.VERTICAL,
            reactive: false,
            style: `background-color: ${css(s.background)}; border-radius: ${s.radius}px; ` +
                `padding: ${s.padding}px; font-family: "${s.font}"; spacing: ${s.size * 0.12}px;`,
        });
        rows.concat(next).forEach((row, i) => {
            const line = new St.BoxLayout({
                style: `spacing: ${row.size * 0.45}px;` + (row.next && i === rows.length && rows.length ? ` margin-top: ${s.size * 0.35}px;` : ''),
            });
            if (row.arrows || row.next) {
                line.add_child(new St.Label({
                    text: row.arrows,
                    y_align: Clutter.ActorAlign.CENTER,
                    style: `color: ${row.arrow}; font-size: ${row.size}px;` + (row.next ? ` min-width: ${column}px;` : ''),
                }));
            }
            if (row.label) {
                line.add_child(new St.Label({
                    text: row.label,
                    y_align: Clutter.ActorAlign.CENTER,
                    style: `color: ${row.text}; font-size: ${row.size}px;`,
                }));
            }
            this._box.add_child(line);
        });
        Main.layoutManager.uiGroup.add_child(this._box);
        this._place();
        this.fade(1);
    }

    // Places the panel on the monitor under the pointer.
    _place() {
        const s = this._style;
        const monitor = Main.layoutManager.currentMonitor;
        const [, width] = this._box.get_preferred_width(-1);
        const [, height] = this._box.get_preferred_height(width);
        const at = (align, size, start, room) => ({
            start: start + s.margin,
            center: start + (room - size) / 2,
            end: start + room - size - s.margin,
        })[align];
        this._box.set_position(
            Math.round(at(s.horizontal, width, monitor.x, monitor.width)),
            Math.round(at(s.vertical, height, monitor.y, monitor.height)));
    }

    fade(factor) {
        if (this._box)
            this._box.opacity = Math.round(255 * this._style.opacity * Math.max(0, factor));
    }

    destroy() {
        this._box?.destroy();
        this._box = null;
    }
}
