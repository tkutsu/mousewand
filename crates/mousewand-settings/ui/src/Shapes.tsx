// Every shape gestures can use: the built-in ones, and the drawn ones,
// which can be redrawn or removed.

import { useState } from "react";
import { deleteShape, type ShapeInfo } from "./api";
import { sentence } from "./check";
import ShapePad from "./ShapePad";
import { Problem, Section, ShapeThumb } from "./ui";

type Props = {
  shapes: ShapeInfo[];
  /** Where the unsaved gestures use a shape, if they do. */
  usedWhere: (name: string) => string | null;
  /** A shape file changed. */
  onChanged: () => void;
};

export default function Shapes({ shapes, usedWhere, onChanged }: Props) {
  // The shape being redrawn, or "" for a new one.
  const [drawing, setDrawing] = useState<string | null>(null);
  // A remove asked for once; asking again removes it.
  const [removing, setRemoving] = useState<string | null>(null);
  const [problem, setProblem] = useState<{ shape: string; text: string } | null>(null);

  const remove = async (name: string) => {
    const where = usedWhere(name);
    if (where) {
      setProblem({ shape: name, text: `A gesture in ${where} uses ${name}. Change that gesture first.` });
      return;
    }
    if (removing !== name) {
      setRemoving(name);
      return;
    }
    try {
      await deleteShape(name);
      onChanged();
    } catch (e) {
      setProblem({ shape: name, text: sentence(e) });
    }
    setRemoving(null);
  };

  const saved = () => {
    setDrawing(null);
    onChanged();
  };

  const builtIn = shapes.filter((s) => s.builtIn);
  const taught = shapes.filter((s) => !s.builtIn);

  return (
    <>
      <Section
        title="Built In"
        note="A drawing matches the shape it looks most like, at any size or tilt, drawn either way round. A stroke with a gesture of its own goes first."
      >
        <div className="grid grid-cols-4 gap-px bg-line">
          {builtIn.map((shape) => (
            <div key={shape.name} className="flex flex-col items-center gap-1.5 bg-group px-2 pt-4 pb-3">
              <ShapeThumb outline={shape.outline} className="size-10 text-accent" />
              <span className="text-[12px] text-ink-2 capitalize">{shape.name}</span>
            </div>
          ))}
        </div>
      </Section>

      <Section
        title="Drawn by You"
        note="Draw a shape five times to teach it. It's saved in shapes/, and any gesture can use it."
        action={
          <button className="button plain h-6 px-2 text-accent" onClick={() => setDrawing("")}>
            <span className="text-[15px] leading-none">+</span> New Shape
          </button>
        }
      >
        {taught.length === 0 && drawing !== "" && <p className="px-3 py-5 text-center text-ink-2">None yet.</p>}
        {taught.map((shape) => (
          <div key={shape.name} className="flex flex-col gap-2 px-3 py-2">
            <div className="flex min-h-9 items-center gap-3">
              <ShapeThumb outline={shape.outline} className="size-8 shrink-0 text-accent" />
              <div className="flex min-w-0 flex-1 flex-col">
                <span>{shape.name}</span>
                <span className="text-[11px] text-ink-2">shapes/{shape.name}.toml</span>
              </div>
              <button className="button" title="Teach it again" onClick={() => setDrawing(shape.name)}>
                Redraw
              </button>
              <button
                className={`button ${removing === shape.name ? "recording" : ""}`}
                title="Click twice to remove"
                onClick={() => remove(shape.name)}
                onBlur={() => setRemoving(null)}
              >
                {removing === shape.name ? "Remove It" : "Remove"}
              </button>
            </div>
            {problem?.shape === shape.name && <Problem>{problem.text}</Problem>}
            {drawing === shape.name && (
              <ShapePad name={shape.name} onSaved={saved} onCancel={() => setDrawing(null)} />
            )}
          </div>
        ))}
        {drawing === "" && (
          <div className="p-3">
            <ShapePad onSaved={saved} onCancel={() => setDrawing(null)} />
          </div>
        )}
      </Section>
    </>
  );
}
