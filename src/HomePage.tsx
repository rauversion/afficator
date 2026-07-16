import {
  ChevronRight,
  Disc3,
  FileAudio2,
  Gauge,
  ListMusic,
  Radio,
  Search,
  Share2,
  Sparkles,
  Tags,
  Upload
} from "lucide-react";
import { Link } from "react-router-dom";
import { Button } from "./components/ui/button";
import { useI18n } from "./i18n";

const homeModules = [
  {
    title: "Rekordbox Convert",
    description: "Importa tu XML, revisa playlists y convierte audio a AIFF sin perder la estructura.",
    to: "/file-conversion/rekordbox-convert",
    icon: FileAudio2,
    accent: "from-violet-500/24 via-fuchsia-500/10 to-transparent",
    iconClass: "bg-violet-500/15 text-violet-700 dark:text-violet-300"
  },
  {
    title: "File Conversion",
    description: "Convierte carpetas y archivos locales con una cola clara, rápida y controlable.",
    to: "/file-conversion/local",
    icon: Upload,
    accent: "from-cyan-500/22 via-sky-500/10 to-transparent",
    iconClass: "bg-cyan-500/15 text-cyan-700 dark:text-cyan-300"
  },
  {
    title: "Playlist Library",
    description: "Indexa, busca y organiza colecciones grandes desde una biblioteca local inteligente.",
    to: "/playlists",
    icon: ListMusic,
    accent: "from-emerald-500/22 via-teal-500/10 to-transparent",
    iconClass: "bg-emerald-500/15 text-emerald-700 dark:text-emerald-300"
  },
  {
    title: "Playlist Copilot",
    description: "Convierte una idea musical en una selección explicable y lista para trabajar.",
    to: "/playlists/copilot",
    icon: Sparkles,
    accent: "from-amber-500/22 via-orange-500/10 to-transparent",
    iconClass: "bg-amber-500/15 text-amber-700 dark:text-amber-300"
  },
  {
    title: "Mastering",
    description: "Prepara masters consistentes con perfiles, análisis y seguimiento del proceso.",
    to: "/mastering",
    icon: Gauge,
    accent: "from-rose-500/22 via-red-500/10 to-transparent",
    iconClass: "bg-rose-500/15 text-rose-700 dark:text-rose-300"
  },
  {
    title: "Turn",
    description: "Transforma y prepara audio con un flujo visual pensado para sesiones largas.",
    to: "/turn",
    icon: Disc3,
    accent: "from-indigo-500/22 via-blue-500/10 to-transparent",
    iconClass: "bg-indigo-500/15 text-indigo-700 dark:text-indigo-300"
  },
  {
    title: "Broadcast",
    description: "Rau Studio reproduce tu cola local y mantiene un stream MP3 persistente hacia Icecast.",
    to: "/broadcast",
    icon: Radio,
    accent: "from-pink-500/22 via-rose-500/10 to-transparent",
    iconClass: "bg-pink-500/15 text-pink-700 dark:text-pink-300"
  },
  {
    title: "Rau Connect",
    description: "Prepara tu identidad y el catálogo que luego viajará directamente entre dispositivos.",
    to: "/connect",
    icon: Share2,
    accent: "from-blue-500/22 via-cyan-500/10 to-transparent",
    iconClass: "bg-blue-500/15 text-blue-700 dark:text-blue-300"
  }
] as const;

export function HomePage() {
  const { t } = useI18n();

  return (
    <main className="relative min-h-screen overflow-hidden p-4 lg:p-6">
      <div
        aria-hidden="true"
        className="pointer-events-none absolute inset-0 opacity-70 dark:opacity-40"
        style={{
          background:
            "radial-gradient(circle at 18% 0%, rgba(139,92,246,.16), transparent 30%), radial-gradient(circle at 92% 18%, rgba(34,211,238,.12), transparent 28%)"
        }}
      />

      <div className="relative mx-auto grid w-full max-w-[1480px] gap-5">
        <section className="relative isolate overflow-hidden rounded-[28px] border border-white/10 bg-slate-950 px-6 py-7 text-white shadow-[0_28px_90px_-42px_rgba(15,23,42,.9)] sm:px-8 lg:grid lg:min-h-[360px] lg:grid-cols-[minmax(0,1.18fr)_minmax(360px,.82fr)] lg:items-center lg:gap-10 lg:px-12 lg:py-10">
          <div
            aria-hidden="true"
            className="absolute inset-0 -z-10 opacity-60"
            style={{
              background:
                "radial-gradient(circle at 15% 20%, rgba(124,58,237,.55), transparent 31%), radial-gradient(circle at 78% 10%, rgba(6,182,212,.32), transparent 29%), linear-gradient(135deg, #020617 18%, #111827 58%, #0f172a)"
            }}
          />
          <div
            aria-hidden="true"
            className="absolute inset-0 -z-10 opacity-[0.14]"
            style={{
              backgroundImage:
                "linear-gradient(rgba(255,255,255,.18) 1px, transparent 1px), linear-gradient(90deg, rgba(255,255,255,.18) 1px, transparent 1px)",
              backgroundSize: "44px 44px",
              maskImage: "linear-gradient(to right, black, transparent 78%)"
            }}
          />

          <div className="max-w-3xl">
            <div className="mb-6 inline-flex items-center gap-2 rounded-full border border-white/15 bg-white/[0.07] px-3 py-1.5 text-xs font-semibold text-white/80 backdrop-blur">
              <span className="relative flex h-2 w-2">
                <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-cyan-300 opacity-70" />
                <span className="relative inline-flex h-2 w-2 rounded-full bg-cyan-300" />
              </span>
              {t("Creative audio workspace · local first")}
            </div>
            <h1 className="m-0 max-w-3xl text-4xl font-semibold leading-[1.02] tracking-[-0.045em] sm:text-5xl lg:text-6xl">
              {t("Tu música, lista para moverse.")}
            </h1>
            <p className="mt-5 max-w-2xl text-sm leading-6 text-slate-300 sm:text-base">
              {t("Convierte, organiza, descubre y finaliza tu catálogo desde un solo estudio privado en tu Mac.")}
            </p>
            <div className="mt-7 flex flex-wrap gap-3">
              <Button asChild className="h-11 rounded-full bg-white px-5 text-slate-950 hover:bg-slate-100">
                <Link to="/file-conversion/rekordbox-convert">
                  <FileAudio2 className="h-4 w-4" />
                  {t("Abrir Rekordbox Convert")}
                </Link>
              </Button>
              <Button asChild variant="secondary" className="h-11 rounded-full border border-white/15 bg-white/10 px-5 text-white hover:bg-white/15">
                <Link to="/playlists">
                  <Search className="h-4 w-4" />
                  {t("Explorar playlists")}
                </Link>
              </Button>
            </div>
          </div>

          <div className="relative mt-10 hidden min-h-[260px] lg:block" aria-hidden="true">
            <div className="absolute left-10 top-0 w-[82%] rotate-[-4deg] rounded-2xl border border-white/10 bg-white/[0.065] p-4 opacity-70 shadow-2xl backdrop-blur-xl">
              <HomeSignal color="bg-violet-400" label="COLLECTION" width="74%" />
              <HomeSignal color="bg-cyan-300" label="PLAYLISTS" width="58%" />
              <HomeSignal color="bg-fuchsia-400" label="MASTER" width="88%" />
            </div>
            <div className="absolute bottom-1 right-0 w-[88%] rotate-[3deg] rounded-2xl border border-white/15 bg-slate-900/85 p-5 shadow-2xl backdrop-blur-xl">
              <div className="flex items-center justify-between">
                <div>
                  <span className="block text-[10px] font-semibold uppercase tracking-[0.22em] text-cyan-300">Rau Studio</span>
                  <strong className="mt-1 block text-base">{t("Flujo conectado")}</strong>
                </div>
                <span className="grid h-11 w-11 place-items-center rounded-full border border-white/10 bg-white/10">
                  <img src="/rau-logo.png" alt="" className="h-8 w-8 object-contain" />
                </span>
              </div>
              <div className="mt-5 grid grid-cols-3 gap-2">
                {["XML", "AIFF", "PLAYLIST"].map((label, index) => (
                  <div key={label} className="rounded-xl border border-white/10 bg-white/[0.055] p-3">
                    <span className="block text-[10px] text-slate-400">0{index + 1}</span>
                    <strong className="mt-2 block text-[11px] tracking-wide text-slate-200">{label}</strong>
                  </div>
                ))}
              </div>
            </div>
          </div>
        </section>

        <section>
          <div className="mb-3 flex flex-wrap items-end justify-between gap-2 px-1">
            <div>
              <span className="text-[11px] font-semibold uppercase tracking-[0.2em] text-muted-foreground">{t("Workspace")}</span>
              <h2 className="mt-1 text-xl font-semibold tracking-tight">{t("Elige por dónde empezar")}</h2>
            </div>
            <span className="text-xs text-muted-foreground">{t("Todo ocurre localmente en tu equipo")}</span>
          </div>

          <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-3">
            {homeModules.map((module) => {
              const Icon = module.icon;
              return (
                <Link
                  key={module.to}
                  to={module.to}
                  className="group relative min-h-44 overflow-hidden rounded-2xl border border-border bg-card p-5 text-card-foreground shadow-sm transition duration-300 hover:-translate-y-1 hover:border-foreground/20 hover:shadow-xl focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                >
                  <div className={`pointer-events-none absolute inset-0 bg-gradient-to-br ${module.accent} opacity-70 transition-opacity group-hover:opacity-100`} />
                  <div className="relative flex h-full flex-col">
                    <div className="flex items-start justify-between gap-3">
                      <span className={`grid h-11 w-11 place-items-center rounded-xl ${module.iconClass}`}>
                        <Icon className="h-5 w-5" />
                      </span>
                      <ChevronRight className="h-4 w-4 text-muted-foreground transition-transform group-hover:translate-x-1 group-hover:text-foreground" />
                    </div>
                    <h3 className="mt-5 text-base font-semibold">{t(module.title)}</h3>
                    <p className="mt-2 max-w-sm text-sm leading-5 text-muted-foreground">{t(module.description)}</p>
                  </div>
                </Link>
              );
            })}
          </div>
        </section>

        <Link
          to="/enrichment"
          className="group flex flex-wrap items-center justify-between gap-4 rounded-2xl border border-border bg-card px-5 py-4 text-card-foreground transition-colors hover:bg-secondary/60 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        >
          <div className="flex min-w-0 items-center gap-4">
            <span className="grid h-11 w-11 shrink-0 place-items-center rounded-xl bg-fuchsia-500/12 text-fuchsia-700 dark:text-fuchsia-300">
              <Tags className="h-5 w-5" />
            </span>
            <div className="min-w-0">
              <strong className="block text-sm">{t("Enriquece tu catálogo")}</strong>
              <span className="mt-1 block text-xs text-muted-foreground">{t("Completa metadata y mejora todo lo que viene después.")}</span>
            </div>
          </div>
          <span className="inline-flex items-center gap-1 text-xs font-semibold">
            {t("Abrir Enrichment")}
            <ChevronRight className="h-4 w-4 transition-transform group-hover:translate-x-1" />
          </span>
        </Link>
      </div>
    </main>
  );
}

function HomeSignal({ color, label, width }: { color: string; label: string; width: string }) {
  return (
    <div className="mb-3 grid grid-cols-[72px_minmax(0,1fr)] items-center gap-3 last:mb-0">
      <span className="text-[9px] font-semibold tracking-[0.16em] text-slate-400">{label}</span>
      <span className="h-1.5 overflow-hidden rounded-full bg-white/10">
        <span className={`block h-full rounded-full ${color}`} style={{ width }} />
      </span>
    </div>
  );
}
