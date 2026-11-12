// The hint panel, drawn by GNOME Shell: the stroke so far as arrows, what
// it could still become, and what fired, how many times. It follows
// mousewand's own hint, with St labels instead of a painted image, and fades
// with the trail.

import Cairo from 'cairo';
import Clutter from 'gi://Clutter';
import St from 'gi://St';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';

const ARROWS = {U: '↑', D: '↓', L: '←', R: '→', 7: '↖', 9: '↗', 1: '↙', 3: '↘'};
// The way each wheel mouse turns.
const WHEEL = {'wheel up': 'U', 'wheel down': 'D', 'wheel left': 'L', 'wheel right': 'R'};

const DEFAULT_STYLE = {
    enabled: false,
    vertical: 'end',
    horizontal: 'center',
    margin: 64,
    font: 'sans-serif',
    size: 17,
    text: [0.95, 0.95, 0.97, 1],
    glyph: [0.62, 0.77, 1, 1],
    failed: [1, 0.35, 0.35, 1],
    background: [0.11, 0.12, 0.15, 1],
    opacity: 0.9,
    padding: 14,
    radius: 14,
};

const css = ([r, g, b, a], alpha = 1) =>
    `rgba(${Math.round(r * 255)}, ${Math.round(g * 255)}, ${Math.round(b * 255)}, ${a * alpha})`;
const arrows = stroke => [...stroke].map(c => ARROWS[c] ?? '').join(' ');

// A shape's outline, fitted into an arrow's square, in a finer line so its
// detail stays open.
function shapeArea(outline, size, [r, g, b, a]) {
    const xs = outline.map(p => p[0]);
    const ys = outline.map(p => p[1]);
    const [x0, x1, y0, y1] = [Math.min(...xs), Math.max(...xs), Math.min(...ys), Math.max(...ys)];
    const span = Math.max(x1 - x0, y1 - y0) || 1;
    const [cx, cy] = [(x0 + x1) / 2, (y0 + y1) / 2];
    const area = new St.DrawingArea({
        y_align: Clutter.ActorAlign.CENTER,
        style: `width: ${size}px; height: ${size}px;`,
    });
    area.connect('repaint', () => {
        const cr = area.get_context();
        const [w] = area.get_surface_size();
        const at = (x, y) => [w * (0.5 + (x - cx) / span * 0.84), w * (0.5 + (y - cy) / span * 0.84)];
        outline.forEach(([x, y], i) => i ? cr.lineTo(...at(x, y)) : cr.moveTo(...at(x, y)));
        cr.setSourceRGBA(r, g, b, a);
        cr.setLineWidth(w * 0.09);
        cr.setLineCap(Cairo.LineCap.ROUND);
        cr.setLineJoin(Cairo.LineJoin.ROUND);
        cr.stroke();
        cr.$dispose();
    });
    return area;
}

function roundedRect(cr, x, y, w, h, r) {
    cr.newSubPath();
    cr.arc(x + w - r, y + r, r, -Math.PI / 2, 0);
    cr.arc(x + w - r, y + h - r, r, 0, Math.PI / 2);
    cr.arc(x + r, y + h - r, r, Math.PI / 2, Math.PI);
    cr.arc(x + r, y + r, r, Math.PI, Math.PI * 1.5);
    cr.closePath();
}

// A wheel or click gesture's mouse in an arrow's square: its outline faint
// and the part the gesture uses solid, a button or the wheel. A wheel's
// arrow follows it, close.
function mouseMark(mouse, size, [r, g, b, a], arrow) {
    const area = new St.DrawingArea({
        y_align: Clutter.ActorAlign.CENTER,
        style: `width: ${size}px; height: ${size}px;`,
    });
    area.connect('repaint', () => {
        const cr = area.get_context();
        const [w] = area.get_surface_size();
        cr.scale(w, w);
        cr.setLineCap(Cairo.LineCap.ROUND);
        cr.setLineJoin(Cairo.LineJoin.ROUND);
        // The body, and the line under its buttons.
        const [x0, y0, bw, bh, br, buttons] = [0.2, 0.04, 0.6, 0.92, 0.24, 0.42];
        roundedRect(cr, x0, y0, bw, bh, br);
        cr.moveTo(x0, buttons);
        cr.lineTo(x0 + bw, buttons);
        cr.moveTo(0.5, y0);
        cr.lineTo(0.5, buttons);
        cr.setSourceRGBA(r, g, b, a * 0.6);
        cr.setLineWidth(0.08);
        cr.stroke();
        if (mouse in WHEEL) {
            roundedRect(cr, 0.43, 0.12, 0.14, 0.22, 0.07);
        } else {
            // The left button; the right one is its mirror image.
            if (mouse === 'click right') {
                cr.translate(1, 0);
                cr.scale(-1, 1);
            }
            cr.moveTo(0.5, y0);
            cr.arcNegative(x0 + br, y0 + br, br, Math.PI * 1.5, Math.PI);
            cr.lineTo(x0, buttons);
            cr.lineTo(0.5, buttons);
            cr.closePath();
        }
        cr.setSourceRGBA(r, g, b, a);
        cr.fill();
        cr.$dispose();
    });
    const box = new St.BoxLayout();
    box.add_child(area);
    if (mouse in WHEEL) {
        box.add_child(new St.Label({
            text: ARROWS[WHEEL[mouse]],
            y_align: Clutter.ActorAlign.CENTER,
            style: `color: ${arrow}; font-size: ${size}px; margin-left: ${-size * 0.25}px;`,
        }));
    }
    return box;
}

// How many arrow squares a row's mark takes.
const squares = row => row.mouse
    ? (row.mouse in WHEEL ? 1.75 : 1)
    : [...row.arrows].filter(c => c !== ' ').length;

export class Hint {
    constructor() {
        this._style = DEFAULT_STYLE;
        this._box = null;
    }

    setStyle(style) {
        this._style = {...DEFAULT_STYLE, ...style};
    }

    // `hint`: {stroke, label, shape: [[x, y]] | null, mouse: string | null, count,
    // state: 'drawing' | 'fired' | 'missed', next: [[rest, label, mouse | null]]}.
    // `shape` is the outline of a shape that fired, `mouse` the wheel or click
    // ('wheel up', 'click left'...), and `count` how many times in a row it fired.
    show(hint) {
        const s = this._style;
        if (!s.enabled)
            return;
        this.destroy();
        // A miss shows only its arrows, in the failed color.
        const [arrow, text, label] = {
            drawing: [css(s.glyph), css(s.text, 0.75), hint.label],
            fired: [css(s.glyph), css(s.text), hint.label],
            missed: [css(s.failed), css(s.failed), hint.label],
        }[hint.state];
        const main = hint.stroke || label
            ? {
                arrows: arrows(hint.stroke), shape: hint.shape, mouse: hint.mouse, label,
                count: hint.count > 1 ? `${hint.count}×` : null,
                size: s.size, glyph: s.glyph, arrow, text,
            }
            : null;
        const next = hint.next.map(([rest, l, mouse]) => ({
            arrows: arrows(rest), mouse, label: l, size: s.size * 0.82,
            glyph: [...s.glyph.slice(0, 3), s.glyph[3] * 0.85],
            arrow: css(s.glyph, 0.85), text: css(s.text, 0.7), next: true,
        }));
        // At the bottom of the screen there's no room under the main line.
        const above = s.vertical === 'end';
        const rows = above ? [...next, main] : [main, ...next];
        if (!main && next.length === 0)
            return;
        // Next moves line their labels up after the longest stroke.
        const column = Math.max(0, ...next.map(squares)) * s.size * 0.82 * 1.05;

        this._box = new St.BoxLayout({
            orientation: Clutter.Orientation.VERTICAL,
            reactive: false,
            style: `background-color: ${css(s.background)}; border-radius: ${s.radius}px; ` +
                `padding: ${s.padding}px; font-family: "${s.font}"; spacing: ${s.size * 0.12}px;`,
        });
        this._main = null;
        rows.filter(Boolean).forEach((row, i, shown) => {
            // A gap between the main line and the next moves.
            const after = i > 0 && row.next !== shown[i - 1].next;
            const line = new St.BoxLayout({
                style: `spacing: ${row.size * 0.45}px;` + (after ? ` margin-top: ${s.size * 0.35}px;` : ''),
            });
            if (!row.next)
                this._main = line;
            // Only a fired hint has a shape, so it's in the glyph color.
            if (row.shape) {
                line.add_child(shapeArea(row.shape, row.size, s.glyph));
            } else if (row.mouse) {
                const mark = mouseMark(row.mouse, row.size, row.glyph, row.arrow);
                if (row.next)
                    mark.style = `min-width: ${column}px;`;
                line.add_child(mark);
            } else if (row.arrows || row.next) {
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
            // In the arrows' color, so it reads as part of the gesture.
            if (row.count) {
                line.add_child(new St.Label({
                    text: row.count,
                    y_align: Clutter.ActorAlign.CENTER,
                    style: `color: ${row.arrow}; font-size: ${row.size}px;`,
                }));
            }
            this._box.add_child(line);
        });
        Main.layoutManager.uiGroup.add_child(this._box);
        this._place(above);
        this.fade(1);
    }

    // Places the panel on the monitor under the pointer, by its main line,
    // so that stays put while the next moves come and go.
    _place(above) {
        const s = this._style;
        const monitor = Main.layoutManager.currentMonitor;
        const [, width] = this._box.get_preferred_width(-1);
        const [, height] = this._box.get_preferred_height(width);
        const [, line] = this._main?.get_preferred_height(width) ?? [0, s.size * 1.45];
        const main = line + s.padding * 2;
        const at = (align, size, start, room) => ({
            start: start + s.margin,
            center: start + (room - size) / 2,
            end: start + room - size - s.margin,
        })[align];
        this._box.set_position(
            Math.round(at(s.horizontal, width, monitor.x, monitor.width)),
            Math.round(at(s.vertical, main, monitor.y, monitor.height) - (above ? height - main : 0)));
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
