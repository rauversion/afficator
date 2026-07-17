import { invoke } from "@tauri-apps/api/core";
import { Check, ListPlus, LoaderCircle, TriangleAlert } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useI18n } from "../../i18n";
import { cn } from "../../lib/utils";
import { Button } from "../ui/button";
import type { TrackListItem } from "./types";

type AddState = "idle" | "adding" | "added" | "error";

export function BroadcastAddButton({
  track,
  className
}: {
  track: Pick<TrackListItem, "library_id" | "track_id" | "source_exists" | "source_path">;
  className?: string;
}) {
  const { t } = useI18n();
  const [state, setState] = useState<AddState>("idle");
  const resetTimer = useRef<number | null>(null);
  const available = Boolean(track.library_id && track.track_id && track.source_exists && track.source_path);
  const label = state === "adding"
    ? t("Agregando al broadcast...")
    : state === "added"
      ? t("Pista agregada al broadcast")
      : state === "error"
        ? t("No se pudo agregar al broadcast")
        : t("Agregar al broadcast");

  useEffect(() => () => {
    if (resetTimer.current !== null) window.clearTimeout(resetTimer.current);
  }, []);

  async function addToBroadcast() {
    if (!available || state === "adding") return;
    if (resetTimer.current !== null) window.clearTimeout(resetTimer.current);
    setState("adding");
    try {
      await invoke("broadcast_append_track", {
        libraryId: track.library_id,
        trackId: track.track_id
      });
      setState("added");
    } catch (error) {
      console.error("Could not add track to broadcast", error);
      setState("error");
    }
    resetTimer.current = window.setTimeout(() => setState("idle"), 2200);
  }

  return (
    <Button
      type="button"
      variant="secondary"
      size="icon"
      className={cn(
        state === "added" && "text-emerald-700 dark:text-emerald-300",
        state === "error" && "text-destructive",
        className
      )}
      disabled={!available || state === "adding"}
      title={label}
      aria-label={label}
      onClick={() => void addToBroadcast()}
    >
      {state === "adding" ? <LoaderCircle className="h-3.5 w-3.5 animate-spin" /> : null}
      {state === "added" ? <Check className="h-3.5 w-3.5" /> : null}
      {state === "error" ? <TriangleAlert className="h-3.5 w-3.5" /> : null}
      {state === "idle" ? <ListPlus className="h-3.5 w-3.5" /> : null}
    </Button>
  );
}
