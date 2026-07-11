import type * as React from "react";
import { Button } from "./ui/button";
import { Card, CardHeader, CardTitle } from "./ui/card";
import { cn } from "../lib/utils";
import { useI18n } from "../i18n";

export type TerminalLogEntry = {
  id: number;
  time: string;
  level: "info" | "warning" | "error";
  track_id?: string;
  name?: string;
  message: string;
};

export function TerminalDrawer({
  logs,
  expanded,
  terminalRef,
  subtitle,
  emptyMessage = "Sin eventos todavia.",
  onToggle,
  onClear
}: {
  logs: TerminalLogEntry[];
  expanded: boolean;
  terminalRef: React.RefObject<HTMLDivElement | null>;
  subtitle: string;
  emptyMessage?: string;
  onToggle: () => void;
  onClear: () => void;
}) {
  const { t } = useI18n();

  return (
    <Card
      className={cn(
        "fixed bottom-3 left-4 right-4 z-50 overflow-hidden shadow-2xl transition-[height] lg:left-[17rem]",
        expanded ? "h-[250px]" : "h-12"
      )}
    >
      <CardHeader className="min-h-12">
        <div className="min-w-0">
          <CardTitle>{t("Terminal")}</CardTitle>
          <span className="block text-xs text-muted-foreground">
            {logs.length} {t("eventos")}
          </span>
        </div>
        <div className="flex items-center gap-2">
          <span className="hidden text-xs text-muted-foreground sm:inline">{subtitle}</span>
          <Button variant="secondary" size="sm" onClick={onToggle}>
            {expanded ? t("Contraer") : t("Expandir")}
          </Button>
          <Button variant="secondary" size="sm" onClick={onClear}>
            {t("Limpiar")}
          </Button>
        </div>
      </CardHeader>
      {expanded ? (
        <div ref={terminalRef} className="h-[calc(250px-48px)] overflow-auto bg-slate-950 px-3 py-2 font-mono text-[11px] leading-relaxed text-slate-200">
          {logs.length === 0 ? <div className="text-slate-500">{t(emptyMessage)}</div> : null}
          {logs.map((log) => (
            <div key={log.id} className={cn("terminal-line", terminalLogClass(log.level))}>
              <span className="truncate text-slate-500">{log.time}</span>
              <span className="truncate font-semibold">{log.level.toUpperCase()}</span>
              <span className="truncate text-slate-400" title={log.track_id ?? ""}>
                {log.name ?? log.track_id ?? "system"}
              </span>
              <span className="whitespace-pre-wrap break-words">{log.message}</span>
            </div>
          ))}
        </div>
      ) : null}
    </Card>
  );
}

function terminalLogClass(level: TerminalLogEntry["level"]) {
  if (level === "error") return "text-red-200";
  if (level === "warning") return "text-amber-200";
  return "text-slate-200";
}
