import { useEffect, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { onEvent, onPetCard, petCardSend } from "@/ipc";
import { useTheme } from "@/hooks/useTheme";
import { PetCard } from "./PetCard";

/**
 * The hover card in its own window (owner 2026-11-01): the same card as before, the same size,
 * placed by the shell beside the pet. The pet window decides when it shows; this window only says
 * whether the pointer is on it and passes Sleep and Wake back.
 */
export function CardApp() {
  useTheme();
  const qc = useQueryClient();
  const [napping, setNapping] = useState(false);
  useEffect(() => onPetCard((m) => { if (m.kind === "napping") setNapping(m.value); }), []);
  // Fresh numbers whenever something changes in Pluto, as in the pet window.
  useEffect(() => onEvent(() => void qc.invalidateQueries()), [qc]);
  return (
    <div
      className="h-screen w-screen select-none overflow-hidden"
      onMouseEnter={() => void petCardSend({ kind: "hover", value: true })}
      onMouseLeave={() => void petCardSend({ kind: "hover", value: false })}
    >
      <PetCard className="size-full" napping={napping} onNap={(n) => { setNapping(n); void petCardSend({ kind: "nap", value: n }); }} />
    </div>
  );
}
