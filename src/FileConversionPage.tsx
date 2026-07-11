import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import {
  CheckCircle2,
  Download,
  FileAudio2,
  FolderOpen,
  Loader2,
  Play,
  RefreshCw,
  Trash2,
  Upload
} from "lucide-react";
import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { Button } from "./components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "./components/ui/card";
import { TerminalDrawer, type TerminalLogEntry } from "./components/terminal-drawer";
import { cn } from "./lib/utils";

type LocalConversionState =
  | "pending"
  | "queued"
  | "running"
  | "converted"
  | "already_converted"
  | "already_aiff"
  | "failed";

type LocalConversionItem = {
  id: string;
  source_path: string;
  source_name: string;
  source_parent: string;
  extension: string;
  target_path: string;
  state: LocalConversionState;
  size_bytes?: number | null;
  modified_ms?: number | null;
  message?: string | null;
  created_at: string;
  updated_at: string;
  completed_at?: string | null;
  source_exists: boolean;
  target_exists: boolean;
};

type LocalConversionImportResponse = {
  group?: LocalConversionGroup | null;
  root_path?: string | null;
  recursive: boolean;
  items: LocalConversionItem[];
  skipped_errors: string[];
};

type LocalConversionGroup = {
  id: string;
  kind: "folder" | "files" | string;
  name: string;
  root_path?: string | null;
  recursive: boolean;
  item_count: number;
  created_at: string;
  updated_at: string;
};

type FileConversionTab = "current" | "all" | "groups";

type LocalConversionProgressEvent = {
  item_id: string;
  name: string;
  source_path: string;
  target_path: string;
  status: LocalConversionState;
  message?: string | null;
  percent?: number | null;
  elapsed_seconds?: number | null;
  speed?: string | null;
};

type LocalConversionLogEvent = {
  level: "info" | "warning" | "error";
  item_id?: string | null;
  name?: string | null;
  message: string;
};

type LocalConversionBatchResult = {
  items: LocalConversionItem[];
  converted_total: number;
  already_converted_total: number;
  already_aiff_total: number;
  failed_total: number;
};

type PlayerState = {
  label: string;
  path: string;
  url: string;
};

const maxConcurrencyLimit = 4;

export function FileConversionPage() {
  const [allItems, setAllItems] = useState<LocalConversionItem[]>([]);
  const [currentItems, setCurrentItems] = useState<LocalConversionItem[]>([]);
  const [groups, setGroups] = useState<LocalConversionGroup[]>([]);
  const [activeGroup, setActiveGroup] = useState<LocalConversionGroup | null>(null);
  const [activeTab, setActiveTab] = useState<FileConversionTab>("all");
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [progressById, setProgressById] = useState<Map<string, LocalConversionProgressEvent>>(new Map());
  const [folderPath, setFolderPath] = useState("");
  const [folderRecursive, setFolderRecursive] = useState(true);
  const [maxConcurrency, setMaxConcurrency] = useState(() => recommendedConcurrencyForCores(detectLogicalCores()));
  const [busy, setBusy] = useState(false);
  const [terminalExpanded, setTerminalExpanded] = useState(false);
  const [terminalLogs, setTerminalLogs] = useState<TerminalLogEntry[]>([]);
  const [player, setPlayer] = useState<PlayerState | null>(null);
  const [message, setMessage] = useState("");
  const [errorMessage, setErrorMessage] = useState("");
  const terminalElement = useRef<HTMLDivElement | null>(null);
  const nextTerminalLogId = useRef(1);

  useEffect(() => {
    void loadItems();

    const unlisteners: UnlistenFn[] = [];
    listen<LocalConversionProgressEvent>("local-conversion-progress", (event) => {
      setProgressById((current) => {
        const next = new Map(current);
        next.set(event.payload.item_id, event.payload);
        return next;
      });
    }).then((unlisten) => unlisteners.push(unlisten));

    listen<LocalConversionLogEvent>("local-conversion-log", (event) => {
      appendTerminalLog(event.payload);
    }).then((unlisten) => unlisteners.push(unlisten));

    return () => {
      for (const unlisten of unlisteners) unlisten();
    };
  }, []);

  const visibleItems = activeTab === "all" ? allItems : currentItems;
  const selectedItems = useMemo(
    () => visibleItems.filter((item) => selectedIds.has(item.id)),
    [visibleItems, selectedIds]
  );
  const processingIds = useMemo(() => {
    const ids = new Set<string>();
    for (const [itemId, progress] of progressById) {
      if (progress.status === "queued" || progress.status === "running") ids.add(itemId);
    }
    return ids;
  }, [progressById]);
  const allSelected = visibleItems.length > 0 && selectedItems.length === visibleItems.length;
  const selectedConvertibleIds = selectedItems
    .filter((item) => canConvert(item, progressById.get(item.id)))
    .map((item) => item.id);
  const stats = useMemo(() => {
    const converted = visibleItems.filter((item) => item.state === "converted" || item.state === "already_converted").length;
    const failed = visibleItems.filter((item) => item.state === "failed").length;
    const missing = visibleItems.filter((item) => !item.source_exists).length;
    return { total: visibleItems.length, converted, failed, missing };
  }, [visibleItems]);
  const activeScopeLabel =
    activeTab === "all"
      ? "Todos los archivos"
      : activeTab === "groups"
        ? "Grupos de importación"
      : activeGroup?.name ?? "Importación actual";
  const activeScopeDetail =
    activeTab === "all"
      ? `${allItems.length} referencia(s) guardadas`
      : activeTab === "groups"
        ? `${groups.length} grupo(s) guardados`
      : `${currentItems.length} archivo(s) en la importación actual`;

  async function loadItems() {
    setErrorMessage("");
    try {
      const [rows, groupRows] = await Promise.all([
        invoke<LocalConversionItem[]>("local_conversion_list_items"),
        invoke<LocalConversionGroup[]>("local_conversion_list_groups")
      ]);
      setAllItems(rows);
      setGroups(groupRows);
      setSelectedIds((current) => new Set(rows.filter((item) => current.has(item.id)).map((item) => item.id)));

      if (activeGroup) {
        const refreshedGroup = groupRows.find((group) => group.id === activeGroup.id);
        if (refreshedGroup) {
          const groupItems = await invoke<LocalConversionItem[]>("local_conversion_group_items", {
            groupId: refreshedGroup.id
          });
          setCurrentItems(groupItems);
          setActiveGroup(refreshedGroup);
        }
      }
    } catch (error) {
      setErrorMessage(String(error));
    }
  }

  async function chooseFiles() {
    setErrorMessage("");
    setMessage("");
    const selected = await open({
      multiple: true,
      filters: [
        {
          name: "Audio",
          extensions: ["wav", "wave", "aif", "aiff", "flac", "mp3", "m4a", "aac", "alac"]
        }
      ]
    });

    const paths = Array.isArray(selected) ? selected : typeof selected === "string" ? [selected] : [];
    if (paths.length === 0) return;

    setBusy(true);
    try {
      const response = await invoke<LocalConversionImportResponse>("local_conversion_add_files", { paths });
      handleImportResponse(response, "archivo(s) agregados");
    } catch (error) {
      setErrorMessage(String(error));
    } finally {
      setBusy(false);
    }
  }

  async function chooseFolder() {
    setErrorMessage("");
    setMessage("");
    const selected = await open({
      multiple: false,
      directory: true
    });

    if (typeof selected !== "string") return;
    setFolderPath(selected);
    await scanFolder(selected);
  }

  async function scanFolder(path = folderPath) {
    if (!path) return;

    setBusy(true);
    setErrorMessage("");
    setMessage("");
    try {
      const response = await invoke<LocalConversionImportResponse>("local_conversion_scan_folder", {
        folderPath: path,
        recursive: folderRecursive
      });
      setFolderPath(response.root_path ?? path);
      handleImportResponse(response, "archivo(s) encontrados");
    } catch (error) {
      setErrorMessage(String(error));
    } finally {
      setBusy(false);
    }
  }

  function handleImportResponse(response: LocalConversionImportResponse, label: string) {
    setAllItems((current) => upsertItems(current, response.items));
    setCurrentItems(response.items);
    setActiveGroup(response.group ?? null);
    if (response.group) setGroups((current) => upsertGroups(current, [response.group]));
    setActiveTab("current");
    setSelectedIds(new Set(response.items.map((item) => item.id)));
    setMessage(`${response.items.length} ${label}.`);
    for (const skipped of response.skipped_errors) {
      appendTerminalLog({
        level: "warning",
        message: skipped
      });
    }
  }

  async function openGroup(group: LocalConversionGroup) {
    setBusy(true);
    setErrorMessage("");
    setMessage("");
    try {
      const rows = await invoke<LocalConversionItem[]>("local_conversion_group_items", {
        groupId: group.id
      });
      setCurrentItems(rows);
      setActiveGroup(group);
      setActiveTab("current");
      setSelectedIds(new Set(rows.map((item) => item.id)));
      if (group.root_path) setFolderPath(group.root_path);
      setFolderRecursive(group.recursive);
      setMessage(`${rows.length} archivo(s) cargados desde ${group.name}.`);
    } catch (error) {
      setErrorMessage(String(error));
    } finally {
      setBusy(false);
    }
  }

  async function convertIds(itemIds: string[]) {
    const uniqueIds = Array.from(new Set(itemIds)).filter((itemId) => !processingIds.has(itemId));
    if (uniqueIds.length === 0) return;

    setBusy(true);
    setErrorMessage("");
    setMessage(`${uniqueIds.length} archivo(s) enviados a conversion.`);
    setProgressById((current) => {
      const next = new Map(current);
      for (const itemId of uniqueIds) {
        const item = visibleItems.find((row) => row.id === itemId) ?? allItems.find((row) => row.id === itemId);
        if (!item) continue;
        next.set(itemId, {
          item_id: item.id,
          name: item.source_name,
          source_path: item.source_path,
          target_path: item.target_path,
          status: "queued",
          message: "En cola",
          percent: 0
        });
      }
      return next;
    });

    try {
      const result = await invoke<LocalConversionBatchResult>("local_conversion_convert_items", {
        itemIds: uniqueIds,
        maxConcurrency
      });
      setAllItems((current) => upsertItems(current, result.items));
      setCurrentItems((current) => {
        const currentIds = new Set(current.map((item) => item.id));
        const relevantItems = result.items.filter((item) => currentIds.has(item.id));
        return relevantItems.length > 0 ? upsertItems(current, relevantItems) : current;
      });
      setMessage(
        `Conversion terminada: ${result.converted_total} convertidos, ${result.already_converted_total} existentes, ${result.failed_total} errores.`
      );
    } catch (error) {
      setErrorMessage(String(error));
    } finally {
      setBusy(false);
    }
  }

  async function deleteItem(item: LocalConversionItem) {
    setErrorMessage("");
    try {
      const deletedId = await invoke<string>("local_conversion_delete_item", { itemId: item.id });
      setAllItems((current) => current.filter((row) => row.id !== deletedId));
      setCurrentItems((current) => current.filter((row) => row.id !== deletedId));
      setSelectedIds((current) => {
        const next = new Set(current);
        next.delete(deletedId);
        return next;
      });
      setProgressById((current) => {
        const next = new Map(current);
        next.delete(deletedId);
        return next;
      });
      void loadItems();
    } catch (error) {
      setErrorMessage(String(error));
    }
  }

  async function openFolderFor(path?: string | null) {
    if (!path) return;
    try {
      await invoke("open_parent_folder", { path });
    } catch (error) {
      setErrorMessage(String(error));
    }
  }

  function toggleSelected(itemId: string) {
    setSelectedIds((current) => {
      const next = new Set(current);
      if (next.has(itemId)) {
        next.delete(itemId);
      } else {
        next.add(itemId);
      }
      return next;
    });
  }

  function toggleAllSelected() {
    setSelectedIds((current) => {
      const next = new Set(current);
      if (allSelected) {
        for (const item of visibleItems) next.delete(item.id);
      } else {
        for (const item of visibleItems) next.add(item.id);
      }
      return next;
    });
  }

  function playPath(path: string, label: string) {
    setPlayer({
      label,
      path,
      url: convertFileSrc(path)
    });
  }

  function appendTerminalLog(log: LocalConversionLogEvent) {
    const nextLog: TerminalLogEntry = {
      id: nextTerminalLogId.current,
      time: new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" }),
      level: log.level,
      track_id: log.item_id ?? undefined,
      name: log.name ?? undefined,
      message: log.message
    };

    nextTerminalLogId.current += 1;
    setTerminalLogs((current) => [...current, nextLog].slice(-1000));
    window.requestAnimationFrame(() => {
      if (terminalElement.current) terminalElement.current.scrollTop = terminalElement.current.scrollHeight;
    });
  }

  function clearTerminal() {
    setTerminalLogs([]);
  }

  return (
    <main className={cn("min-w-0 p-4 pb-20", terminalExpanded && "pb-72")}>
      <header className="mb-3 flex flex-wrap items-center justify-between gap-3 border-b border-border pb-3">
        <div className="flex min-w-0 items-center gap-3">
          <span className="grid h-10 w-10 shrink-0 place-items-center rounded-md border border-border bg-secondary text-secondary-foreground">
            <FileAudio2 className="h-5 w-5" />
          </span>
          <div className="min-w-0">
            <h1 className="m-0 text-2xl font-semibold tracking-normal">File Conversion</h1>
            <p className="mt-1 truncate text-xs text-muted-foreground">
              {folderPath || "Convierte archivos locales a AIFF en carpetas converted."}
            </p>
          </div>
        </div>

        <div className="flex flex-wrap items-center gap-2">
          <Button variant="secondary" onClick={() => void loadItems()} disabled={busy}>
            <RefreshCw className="h-4 w-4" />
            Refrescar
          </Button>
          <Button variant="secondary" onClick={() => void chooseFolder()} disabled={busy}>
            <FolderOpen className="h-4 w-4" />
            Carpeta
          </Button>
          <Button onClick={() => void chooseFiles()} disabled={busy}>
            <Upload className="h-4 w-4" />
            Archivos
          </Button>
        </div>
      </header>

      {errorMessage ? (
        <div className="mb-3 rounded-md border border-red-300 bg-red-50 px-3 py-2 text-sm text-red-800 dark:border-red-900 dark:bg-red-950/50 dark:text-red-200">
          {errorMessage}
        </div>
      ) : null}
      {message ? (
        <div className="mb-3 rounded-md border border-border bg-muted px-3 py-2 text-sm text-foreground">
          {message}
        </div>
      ) : null}

      <section className="grid gap-3 xl:grid-cols-[minmax(0,1fr)_340px]">
        <div className="grid gap-3">
          <Card>
            <CardHeader>
              <CardTitle>Entrada</CardTitle>
              <span className="text-xs text-muted-foreground">{selectedItems.length} seleccionados</span>
            </CardHeader>
            <CardContent className="grid gap-3 p-3">
              <div className="grid gap-2 md:grid-cols-[minmax(0,1fr)_auto_auto]">
                <div className="truncate rounded-md border border-border bg-secondary px-3 py-2 text-sm" title={folderPath}>
                  {folderPath || "Sin carpeta activa"}
                </div>
                <label className="inline-flex h-9 items-center gap-2 rounded-md border border-border bg-background px-3 text-sm font-medium">
                  <input
                    type="checkbox"
                    checked={folderRecursive}
                    onChange={(event) => setFolderRecursive(event.currentTarget.checked)}
                  />
                  Recursivo
                </label>
                <Button variant="secondary" disabled={busy || !folderPath} onClick={() => void scanFolder()}>
                  Escanear
                </Button>
              </div>

              <div className="flex flex-wrap items-center gap-2">
                <label className="grid gap-1 text-xs font-semibold text-muted-foreground">
                  Concurrencia
                  <select
                    className="h-9 rounded-md border border-input bg-background px-2 text-sm text-foreground"
                    value={maxConcurrency}
                    onChange={(event) => setMaxConcurrency(Number(event.currentTarget.value))}
                  >
                    {concurrencyOptionsForCores(detectLogicalCores()).map((value) => (
                      <option key={value} value={value}>
                        {value}
                      </option>
                    ))}
                  </select>
                </label>
                <Button disabled={busy || selectedConvertibleIds.length === 0} onClick={() => void convertIds(selectedConvertibleIds)}>
                  {busy ? <Loader2 className="h-4 w-4 animate-spin" /> : <FileAudio2 className="h-4 w-4" />}
                  Convertir seleccionados
                </Button>
              </div>
            </CardContent>
          </Card>

          <div className="flex flex-wrap items-center justify-between gap-2 rounded-md border border-border bg-card p-2">
            <div className="flex flex-wrap items-center gap-1">
              <TabButton active={activeTab === "current"} onClick={() => setActiveTab("current")}>
                Importación actual
              </TabButton>
              <TabButton active={activeTab === "all"} onClick={() => setActiveTab("all")}>
                Todos
              </TabButton>
              <TabButton active={activeTab === "groups"} onClick={() => setActiveTab("groups")}>
                Grupos
              </TabButton>
            </div>
            <span className="min-w-0 truncate px-2 text-xs text-muted-foreground" title={activeScopeLabel}>
              {activeScopeDetail}
            </span>
          </div>

          {activeTab === "groups" ? (
            <Card className="overflow-hidden">
              <CardHeader>
                <CardTitle>Grupos de importación</CardTitle>
                <span className="text-xs text-muted-foreground">{groups.length} grupo(s) guardados</span>
              </CardHeader>
              <CardContent className="p-0">
                {groups.length === 0 ? (
                  <div className="px-3 py-6 text-sm text-muted-foreground">
                    Todavía no hay grupos. Abre una carpeta o selecciona archivos para crear uno.
                  </div>
                ) : null}
                <div className="divide-y divide-border">
                  {groups.map((group) => (
                    <button
                      key={group.id}
                      type="button"
                      className="grid w-full grid-cols-[32px_minmax(0,1fr)_96px_128px] items-center gap-3 px-3 py-3 text-left text-sm hover:bg-secondary"
                      onClick={() => void openGroup(group)}
                    >
                      <span className="grid h-8 w-8 place-items-center rounded-md border border-border bg-background">
                        {group.kind === "folder" ? <FolderOpen className="h-4 w-4" /> : <Upload className="h-4 w-4" />}
                      </span>
                      <span className="min-w-0">
                        <span className="block truncate font-semibold" title={group.name}>{group.name}</span>
                        <span className="mt-1 block truncate text-xs text-muted-foreground" title={group.root_path ?? undefined}>
                          {group.root_path ?? groupKindLabel(group)}
                        </span>
                      </span>
                      <span className="text-xs text-muted-foreground">{group.item_count} archivo(s)</span>
                      <span className="text-right text-xs text-muted-foreground">{formatShortDate(group.updated_at)}</span>
                    </button>
                  ))}
                </div>
              </CardContent>
            </Card>
          ) : (
          <Card className="overflow-hidden">
            <CardHeader>
              <div className="flex items-center gap-2">
                <input type="checkbox" checked={allSelected} onChange={toggleAllSelected} />
                <CardTitle>{activeScopeLabel}</CardTitle>
              </div>
              <span className="text-xs text-muted-foreground">{visibleItems.length} referencias</span>
            </CardHeader>
            <CardContent className="p-0">
              <div className="overflow-x-auto">
                <div className="min-w-[760px]">
                  <div className="grid grid-cols-[32px_minmax(0,1.4fr)_112px_76px_minmax(0,0.9fr)_174px] gap-2 border-b border-border bg-secondary px-3 py-2 text-xs font-semibold text-muted-foreground">
                    <span />
                    <span>Archivo</span>
                    <span>Estado</span>
                    <span>Tamano</span>
                    <span>Destino</span>
                    <span className="text-right">Acciones</span>
                  </div>
                  {visibleItems.length === 0 ? (
                    <div className="px-3 py-6 text-sm text-muted-foreground">
                      {activeTab === "current"
                        ? "Abre una carpeta o un grupo para ver la importación actual."
                        : "Agrega archivos o escanea una carpeta para empezar."}
                    </div>
                  ) : null}
                  {visibleItems.map((item) => {
                    const progress = progressById.get(item.id);
                    const state = progress?.status ?? item.state;
                    const percent = progress?.percent ?? (isDoneState(state) ? 100 : 0);
                    const processing = state === "queued" || state === "running";
                    const canPlayTarget = item.target_exists || state === "converted" || state === "already_converted" || state === "already_aiff";

                    return (
                      <div key={item.id} className="grid grid-cols-[32px_minmax(0,1.4fr)_112px_76px_minmax(0,0.9fr)_174px] items-center gap-2 border-b border-border px-3 py-2 text-sm">
                        <input
                          type="checkbox"
                          checked={selectedIds.has(item.id)}
                          onChange={() => toggleSelected(item.id)}
                        />
                        <div className="min-w-0">
                          <button
                            type="button"
                            className="block max-w-full truncate text-left font-semibold hover:underline"
                            title={item.source_path}
                            onClick={() => item.source_exists && playPath(item.source_path, item.source_name)}
                          >
                            {item.source_name}
                          </button>
                          <div className="mt-1 truncate text-xs text-muted-foreground" title={item.source_parent}>
                            {item.source_parent}
                          </div>
                          {processing || percent > 0 ? <Progress value={percent} /> : null}
                        </div>
                        <StatusBadge state={state} />
                        <span className="text-xs text-muted-foreground">{formatBytes(item.size_bytes ?? 0)}</span>
                        <span className="truncate text-xs text-muted-foreground" title={item.target_path}>
                          {item.target_path}
                        </span>
                        <div className="flex justify-end gap-1">
                          <Button variant="secondary" size="icon" title="Escuchar original" disabled={!item.source_exists} onClick={() => playPath(item.source_path, item.source_name)}>
                            <Play className="h-3.5 w-3.5" />
                          </Button>
                          <Button variant="secondary" size="icon" title="Escuchar AIFF" disabled={!canPlayTarget} onClick={() => playPath(item.target_path, `${item.source_name} AIFF`)}>
                            <CheckCircle2 className="h-3.5 w-3.5" />
                          </Button>
                          <Button variant="secondary" size="icon" title="Convertir" disabled={!canConvert(item, progress)} onClick={() => void convertIds([item.id])}>
                            {processing ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <FileAudio2 className="h-3.5 w-3.5" />}
                          </Button>
                          <Button variant="secondary" size="icon" title="Abrir carpeta" onClick={() => void openFolderFor(item.target_exists ? item.target_path : item.source_path)}>
                            <FolderOpen className="h-3.5 w-3.5" />
                          </Button>
                          <Button variant="ghost" size="icon" title="Olvidar" disabled={processing} onClick={() => void deleteItem(item)}>
                            <Trash2 className="h-3.5 w-3.5" />
                          </Button>
                        </div>
                      </div>
                    );
                  })}
                </div>
              </div>
            </CardContent>
          </Card>
          )}
        </div>

        <aside className="grid gap-3 content-start">
          <section className="grid grid-cols-2 gap-2">
            <Metric label="Total" value={stats.total} />
            <Metric label="Convertidos" value={stats.converted} />
            <Metric label="Errores" value={stats.failed} danger={stats.failed > 0} />
            <Metric label="Missing" value={stats.missing} danger={stats.missing > 0} />
          </section>

          <Card>
            <CardHeader>
              <CardTitle>Player</CardTitle>
            </CardHeader>
            <CardContent className="grid gap-2 p-3">
              {player ? (
                <>
                  <div className="truncate text-sm font-semibold" title={player.path}>{player.label}</div>
                  <audio className="w-full" controls autoPlay src={player.url} />
                  <div className="break-words font-mono text-[11px] text-muted-foreground">{player.path}</div>
                  <Button variant="secondary" onClick={() => void openFolderFor(player.path)}>
                    <FolderOpen className="h-4 w-4" />
                    Abrir carpeta
                  </Button>
                </>
              ) : (
                <div className="text-sm text-muted-foreground">Selecciona play en una fila.</div>
              )}
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardTitle>Destino</CardTitle>
            </CardHeader>
            <CardContent className="grid gap-2 p-3 text-sm text-muted-foreground">
              <p>Los AIFF se guardan al lado del original, dentro de una carpeta llamada converted.</p>
              <p>No se reemplazan archivos fuente.</p>
              <Button asChild variant="secondary" disabled={!player?.path}>
                <a href={player ? convertFileSrc(player.path) : "#"} download>
                  <Download className="h-4 w-4" />
                  Descargar actual
                </a>
              </Button>
            </CardContent>
          </Card>
        </aside>
      </section>

      <TerminalDrawer
        logs={terminalLogs}
        expanded={terminalExpanded}
        terminalRef={terminalElement}
        subtitle="ffmpeg / file conversion"
        onToggle={() => setTerminalExpanded((current) => !current)}
        onClear={clearTerminal}
      />
    </main>
  );
}

function TabButton({
  active,
  children,
  onClick
}: {
  active: boolean;
  children: ReactNode;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      className={cn(
        "h-8 rounded-md px-3 text-sm font-medium text-muted-foreground hover:bg-secondary hover:text-foreground",
        active && "bg-primary text-primary-foreground hover:bg-primary hover:text-primary-foreground"
      )}
      onClick={onClick}
    >
      {children}
    </button>
  );
}

function Metric({ label, value, danger = false }: { label: string; value: number; danger?: boolean }) {
  return (
    <Card className={cn("p-3", danger && "border-red-300 text-red-800 dark:border-red-900 dark:text-red-200")}>
      <span className="block text-xs text-muted-foreground">{label}</span>
      <strong className="mt-1 block text-xl">{value}</strong>
    </Card>
  );
}

function StatusBadge({ state }: { state: LocalConversionState }) {
  return (
    <span
      className={cn(
        "inline-flex w-fit items-center gap-1 rounded-full px-2 py-0.5 text-[11px] font-semibold",
        (state === "converted" || state === "already_converted" || state === "already_aiff") &&
          "bg-emerald-100 text-emerald-800 dark:bg-emerald-950 dark:text-emerald-200",
        state === "failed" && "bg-red-100 text-red-800 dark:bg-red-950 dark:text-red-200",
        (state === "queued" || state === "running") && "bg-blue-100 text-blue-800 dark:bg-blue-950 dark:text-blue-200",
        state === "pending" && "bg-secondary text-secondary-foreground"
      )}
    >
      {state === "running" || state === "queued" ? <Loader2 className="h-3 w-3 animate-spin" /> : null}
      {stateLabel(state)}
    </span>
  );
}

function Progress({ value }: { value: number }) {
  return (
    <div className="mt-1 h-1.5 overflow-hidden rounded-full bg-secondary">
      <div className="h-full rounded-full bg-primary transition-all" style={{ width: `${Math.max(0, Math.min(100, value))}%` }} />
    </div>
  );
}

function canConvert(item: LocalConversionItem, progress?: LocalConversionProgressEvent) {
  const state = progress?.status ?? item.state;
  return item.source_exists && state !== "queued" && state !== "running" && state !== "already_aiff";
}

function isDoneState(state: LocalConversionState) {
  return state === "converted" || state === "already_converted" || state === "already_aiff";
}

function upsertItems(current: LocalConversionItem[], incoming: LocalConversionItem[]) {
  const byId = new Map(current.map((item) => [item.id, item]));
  for (const item of incoming) byId.set(item.id, item);
  return Array.from(byId.values()).sort((left, right) => right.updated_at.localeCompare(left.updated_at));
}

function upsertGroups(current: LocalConversionGroup[], incoming: LocalConversionGroup[]) {
  const byId = new Map(current.map((group) => [group.id, group]));
  for (const group of incoming) byId.set(group.id, group);
  return Array.from(byId.values()).sort((left, right) => right.updated_at.localeCompare(left.updated_at));
}

function stateLabel(state: LocalConversionState) {
  const labels: Record<LocalConversionState, string> = {
    queued: "en cola",
    pending: "pendiente",
    running: "procesando",
    converted: "convertido",
    already_converted: "existente",
    already_aiff: "AIFF",
    failed: "error"
  };
  return labels[state] ?? state;
}

function formatBytes(bytes: number) {
  if (!Number.isFinite(bytes) || bytes <= 0) return "n/d";
  if (bytes >= 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
  if (bytes >= 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  if (bytes >= 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${bytes} B`;
}

function groupKindLabel(group: LocalConversionGroup) {
  if (group.kind === "folder") return group.recursive ? "Carpeta recursiva" : "Carpeta";
  return "Selección manual";
}

function formatShortDate(value: string) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString([], {
    day: "2-digit",
    month: "2-digit",
    hour: "2-digit",
    minute: "2-digit"
  });
}

function detectLogicalCores() {
  if (typeof navigator === "undefined") return 1;
  const cores = navigator.hardwareConcurrency;
  return Number.isFinite(cores) && cores > 0 ? Math.floor(cores) : 1;
}

function recommendedConcurrencyForCores(cores: number) {
  return Math.max(1, Math.min(maxConcurrencyLimit, Math.floor(cores / 2) || 1));
}

function concurrencyOptionsForCores(cores: number) {
  const max = Math.max(1, Math.min(maxConcurrencyLimit, cores));
  return Array.from({ length: max }, (_, index) => index + 1);
}
