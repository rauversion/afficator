import { ListMusic, Plus } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import type * as React from "react";
import { useI18n } from "../../i18n";
import { Button } from "../ui/button";

export type PlaylistDraftOption = {
  id: string;
  name: string;
  track_count: number;
};

type PlaylistAddMode = "existing" | "new";

export function PlaylistAddDialog({
  busy,
  contextLabel,
  defaultName,
  drafts,
  open,
  trackCount,
  onAddExisting,
  onClose,
  onCreate
}: {
  busy: boolean;
  contextLabel: string;
  defaultName: string;
  drafts: PlaylistDraftOption[];
  open: boolean;
  trackCount: number;
  onAddExisting: (draftId: string) => void;
  onClose: () => void;
  onCreate: (name: string, description: string) => void;
}) {
  const { t } = useI18n();
  const initialMode = drafts.length > 0 ? "existing" : "new";
  const [mode, setMode] = useState<PlaylistAddMode>(initialMode);
  const [targetDraftId, setTargetDraftId] = useState(drafts[0]?.id ?? "");
  const [name, setName] = useState(defaultName);
  const [description, setDescription] = useState("");

  useEffect(() => {
    if (!open) return;
    setMode(drafts.length > 0 ? "existing" : "new");
    setTargetDraftId(drafts[0]?.id ?? "");
    setName(defaultName);
    setDescription("");
  }, [defaultName, drafts, open]);

  const selectedDraftId = useMemo(() => {
    if (drafts.some((draft) => draft.id === targetDraftId)) return targetDraftId;
    return drafts[0]?.id ?? "";
  }, [drafts, targetDraftId]);

  if (!open) return null;

  function submitExisting(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!selectedDraftId || trackCount === 0) return;
    onAddExisting(selectedDraftId);
  }

  function submitNew(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!name.trim() || trackCount === 0) return;
    onCreate(name.trim(), description.trim());
  }

  return (
    <div className="fixed inset-0 z-[65] flex items-center justify-center p-4" role="dialog" aria-modal="true">
      <div className="absolute inset-0 bg-black/35 backdrop-blur-[1px]" onClick={onClose} />
      <section className="relative z-[70] w-full max-w-lg rounded-md border border-border bg-background text-foreground shadow-2xl">
        <header className="flex items-start justify-between gap-3 border-b border-border bg-card px-4 py-4">
          <div className="min-w-0">
            <h2 className="truncate text-base font-semibold">{t("Agregar a playlist")}</h2>
            <p className="mt-1 text-sm text-muted-foreground">
              {t("Vas a agregar {count} tracks.", { count: trackCount })}
            </p>
          </div>
          <Button variant="ghost" size="sm" onClick={onClose}>
            {t("Cerrar")}
          </Button>
        </header>

        <div className="grid gap-4 p-4">
          <div className="rounded-md border border-border bg-secondary/60 p-3 text-sm">
            <strong className="block truncate">{contextLabel}</strong>
            <span className="mt-1 block text-xs text-muted-foreground">
              {trackCount} {t("tracks seleccionados")}
            </span>
          </div>

          <div className="grid grid-cols-2 gap-1 rounded-md border border-border bg-card p-1">
            <Button
              type="button"
              variant={mode === "existing" ? "default" : "ghost"}
              disabled={drafts.length === 0}
              onClick={() => setMode("existing")}
            >
              <ListMusic className="h-4 w-4" />
              {t("Existente")}
            </Button>
            <Button type="button" variant={mode === "new" ? "default" : "ghost"} onClick={() => setMode("new")}>
              <Plus className="h-4 w-4" />
              {t("Nueva")}
            </Button>
          </div>

          {mode === "existing" ? (
            <form className="grid gap-3" onSubmit={submitExisting}>
              {drafts.length === 0 ? (
                <div className="rounded-md border border-border bg-muted p-3 text-sm text-muted-foreground">
                  {t("No hay playlists nuevas. Crea una playlist para agregar estos tracks.")}
                </div>
              ) : null}
              <label className="grid gap-1 text-sm">
                <span className="font-semibold">{t("Playlist")}</span>
                <select
                  className="h-10 rounded-md border border-input bg-background px-3 outline-none focus-visible:ring-2 focus-visible:ring-ring"
                  value={selectedDraftId}
                  onChange={(event) => setTargetDraftId(event.currentTarget.value)}
                  disabled={drafts.length === 0}
                >
                  {drafts.map((draft) => (
                    <option key={draft.id} value={draft.id}>
                      {draft.name} ({draft.track_count})
                    </option>
                  ))}
                </select>
              </label>
              <div className="flex justify-end gap-2">
                <Button type="button" variant="secondary" onClick={onClose}>
                  {t("Cancelar")}
                </Button>
                <Button disabled={busy || !selectedDraftId || trackCount === 0}>
                  <ListMusic className="h-4 w-4" />
                  {t("Agregar {count} tracks", { count: trackCount })}
                </Button>
              </div>
            </form>
          ) : (
            <form className="grid gap-3" onSubmit={submitNew}>
              <label className="grid gap-1 text-sm">
                <span className="font-semibold">{t("Nombre")}</span>
                <input
                  className="h-10 rounded-md border border-input bg-background px-3 outline-none focus-visible:ring-2 focus-visible:ring-ring"
                  value={name}
                  onChange={(event) => setName(event.currentTarget.value)}
                />
              </label>
              <label className="grid gap-1 text-sm">
                <span className="font-semibold">{t("Descripcion")}</span>
                <textarea
                  className="min-h-24 rounded-md border border-input bg-background px-3 py-2 outline-none focus-visible:ring-2 focus-visible:ring-ring"
                  value={description}
                  onChange={(event) => setDescription(event.currentTarget.value)}
                />
              </label>
              <div className="flex justify-end gap-2">
                <Button type="button" variant="secondary" onClick={onClose}>
                  {t("Cancelar")}
                </Button>
                <Button disabled={busy || !name.trim() || trackCount === 0}>
                  <Plus className="h-4 w-4" />
                  {t("Crear y agregar {count} tracks", { count: trackCount })}
                </Button>
              </div>
            </form>
          )}
        </div>
      </section>
    </div>
  );
}
