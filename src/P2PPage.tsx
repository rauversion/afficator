import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import {
  Activity,
  Copy,
  Download,
  FolderOpen,
  Globe2,
  HardDrive,
  KeyRound,
  Library,
  LoaderCircle,
  LockKeyhole,
  MessageCircle,
  Network,
  Pause,
  Play,
  Radio,
  RefreshCcw,
  Search,
  Send,
  ShieldCheck,
  Trash2,
  UsersRound,
  Wifi,
  WifiOff
} from "lucide-react";
import { useCallback, useEffect, useMemo, useState, type FormEvent, type ReactNode } from "react";
import { Button } from "./components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "./components/ui/card";
import { useI18n } from "./i18n";
import { cn } from "./lib/utils";

type IdentityStatus = {
  configured: boolean;
  unlocked: boolean;
  display_name?: string | null;
  endpoint_id?: string | null;
};

type PeerSummary = {
  endpoint_id: string;
  display_name: string;
  trust_state: string;
  presence_status: "online" | "away" | "offline" | "unknown" | string;
  last_seen_at?: string | null;
  can_connect: boolean;
};

type NetworkStatus = {
  running: boolean;
  endpoint_id?: string | null;
  ticket?: string | null;
  relay_ready: boolean;
  address_count: number;
  bound_sockets: string[];
  started_at?: string | null;
};

type NetworkEvent = {
  kind: "started" | "stopped" | "incoming_ping" | "ping_succeeded" | string;
  peer_endpoint_id?: string | null;
  peer_display_name?: string | null;
  message: string;
  rtt_ms?: number | null;
  occurred_at: string;
};

type PingResult = {
  remote_endpoint_id: string;
  remote_display_name: string;
  rtt_ms: number;
  protocol_version: number;
  received_at: string;
};

type SharedFolder = {
  id: string;
  name: string;
  root_path: string;
  visibility: "contacts" | "selected_contacts" | "community" | "ticket";
  enabled: boolean;
  file_count: number;
  total_size_bytes: number;
  skipped_entries: number;
  last_indexed_at: string;
  created_at: string;
};

type SharedFileSearchResult = {
  provider_endpoint_id: string;
  share_id: string;
  share_name: string;
  file_id: string;
  name: string;
  relative_path: string;
  extension: string;
  size_bytes: number;
  modified_ms?: number | null;
};

type SharedFileSearchResponse = {
  query: string;
  results: SharedFileSearchResult[];
};

type RemoteCatalogResponse = SharedFileSearchResponse & {
  peer_endpoint_id: string;
  peer_display_name: string;
};

type DownloadResult = {
  peer_endpoint_id: string;
  name: string;
  destination_path: string;
  size_bytes: number;
  elapsed_ms: number;
};

type TransferEvent = {
  transfer_id: string;
  peer_endpoint_id: string;
  file_name: string;
  destination_path: string;
  received_bytes: number;
  total_bytes: number;
  status: "downloading" | "completed" | string;
  message: string;
  occurred_at: string;
};

type ChatRoom = "private" | "general";

type ChatMessage = {
  id: string;
  room: ChatRoom;
  peer_endpoint_id: string;
  sender_endpoint_id: string;
  sender_display_name: string;
  body: string;
  direction: "incoming" | "outgoing" | string;
  delivery_status: "pending" | "delivered" | "partial" | "failed" | string;
  sent_at: string;
  received_at?: string | null;
};

type ChatEvent = {
  kind: "message_received" | "message_sent" | string;
  message: ChatMessage;
};

type ChatSendResult = {
  message: ChatMessage;
  attempted_recipients: number;
  delivered_recipients: number;
};

type BusyAction =
  | "loading"
  | "identity"
  | "unlock"
  | "lock"
  | "network"
  | "ping"
  | "add-share"
  | `share:${string}`
  | "search"
  | "remote-search"
  | `download:${string}`
  | "chat-send"
  | null;

const fieldClass =
  "h-10 w-full rounded-md border border-border bg-background px-3 text-sm text-foreground outline-none transition focus:border-foreground/35 focus:ring-2 focus:ring-ring/30 disabled:cursor-not-allowed disabled:opacity-60";

export function P2PPage() {
  const { t } = useI18n();
  const [identity, setIdentity] = useState<IdentityStatus | null>(null);
  const [peers, setPeers] = useState<PeerSummary[]>([]);
  const [shares, setShares] = useState<SharedFolder[]>([]);
  const [network, setNetwork] = useState<NetworkStatus | null>(null);
  const [networkEvents, setNetworkEvents] = useState<NetworkEvent[]>([]);
  const [remoteTicket, setRemoteTicket] = useState("");
  const [pingResult, setPingResult] = useState<PingResult | null>(null);
  const [busy, setBusy] = useState<BusyAction>("loading");
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const [displayName, setDisplayName] = useState("");
  const [newPassword, setNewPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const [unlockPassword, setUnlockPassword] = useState("");

  const [selectedPath, setSelectedPath] = useState("");
  const [shareName, setShareName] = useState("");
  const [visibility, setVisibility] = useState<SharedFolder["visibility"]>("contacts");
  const [searchQuery, setSearchQuery] = useState("");
  const [searchResults, setSearchResults] = useState<SharedFileSearchResult[]>([]);
  const [hasSearched, setHasSearched] = useState(false);
  const [remotePeer, setRemotePeer] = useState("");
  const [remoteQuery, setRemoteQuery] = useState("");
  const [remoteResults, setRemoteResults] = useState<SharedFileSearchResult[]>([]);
  const [hasRemoteSearched, setHasRemoteSearched] = useState(false);
  const [transfer, setTransfer] = useState<TransferEvent | null>(null);
  const [chatRoom, setChatRoom] = useState<ChatRoom>("private");
  const [chatPeer, setChatPeer] = useState("");
  const [chatMessages, setChatMessages] = useState<ChatMessage[]>([]);
  const [chatBody, setChatBody] = useState("");

  const refresh = useCallback(async () => {
    setError(null);
    const [nextIdentity, nextPeers, nextShares, nextNetwork] = await Promise.all([
      invoke<IdentityStatus>("p2p_identity_status"),
      invoke<PeerSummary[]>("p2p_list_peers"),
      invoke<SharedFolder[]>("p2p_list_shares"),
      invoke<NetworkStatus>("p2p_network_status")
    ]);
    setIdentity(nextIdentity);
    setPeers(nextPeers);
    setShares(nextShares);
    setNetwork(nextNetwork);
  }, []);

  useEffect(() => {
    void refresh()
      .catch((cause) => setError(errorMessage(cause)))
      .finally(() => setBusy(null));
  }, [refresh]);

  useEffect(() => {
    let disposed = false;
    let unlisten: UnlistenFn | undefined;

    void listen<NetworkEvent>("p2p-network-event", ({ payload }) => {
      setNetworkEvents((current) => [payload, ...current].slice(0, 6));
      void Promise.all([
        invoke<NetworkStatus>("p2p_network_status").then(setNetwork),
        invoke<PeerSummary[]>("p2p_list_peers").then(setPeers)
      ]).catch(() => undefined);
    })
      .then((stopListening) => {
        if (disposed) stopListening();
        else unlisten = stopListening;
      })
      .catch(() => undefined);

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    if (!network?.running) return;
    const interval = window.setInterval(() => {
      void Promise.all([
        invoke<NetworkStatus>("p2p_network_status").then(setNetwork),
        invoke<PeerSummary[]>("p2p_list_peers").then(setPeers)
      ]).catch(() => undefined);
    }, 3000);
    return () => window.clearInterval(interval);
  }, [network?.running]);

  const sharedFileCount = useMemo(
    () => shares.reduce((total, share) => total + share.file_count, 0),
    [shares]
  );
  const reachablePeers = useMemo(() => peers.filter((peer) => peer.can_connect), [peers]);

  useEffect(() => {
    const firstPeer = reachablePeers[0]?.endpoint_id ?? "";
    if (!reachablePeers.some((peer) => peer.endpoint_id === remotePeer)) {
      setRemotePeer(firstPeer);
      setRemoteResults([]);
      setHasRemoteSearched(false);
    }
    if (!reachablePeers.some((peer) => peer.endpoint_id === chatPeer)) {
      setChatPeer(firstPeer);
    }
  }, [chatPeer, reachablePeers, remotePeer]);

  useEffect(() => {
    if (!identity?.unlocked || (chatRoom === "private" && !chatPeer)) {
      setChatMessages([]);
      return;
    }
    void invoke<ChatMessage[]>("p2p_chat_list", {
      room: chatRoom,
      peerEndpointId: chatRoom === "private" ? chatPeer : null,
      limit: 300
    })
      .then(setChatMessages)
      .catch((cause) => setError(errorMessage(cause)));
  }, [chatPeer, chatRoom, identity?.unlocked]);

  useEffect(() => {
    let disposed = false;
    let unlisten: UnlistenFn | undefined;
    void listen<ChatEvent>("p2p-chat-event", ({ payload }) => {
      const matchesRoom = payload.message.room === chatRoom;
      const matchesPeer = chatRoom === "general"
        || payload.message.peer_endpoint_id === chatPeer;
      if (matchesRoom && matchesPeer) {
        setChatMessages((current) => upsertChatMessage(current, payload.message));
      }
    })
      .then((stopListening) => {
        if (disposed) stopListening();
        else unlisten = stopListening;
      })
      .catch(() => undefined);
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [chatPeer, chatRoom]);

  useEffect(() => {
    let disposed = false;
    let unlisten: UnlistenFn | undefined;
    void listen<TransferEvent>("p2p-transfer-event", ({ payload }) => setTransfer(payload))
      .then((stopListening) => {
        if (disposed) stopListening();
        else unlisten = stopListening;
      })
      .catch(() => undefined);
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  async function createIdentity(event: FormEvent) {
    event.preventDefault();
    setError(null);
    setNotice(null);
    if (newPassword !== confirmPassword) {
      setError(t("Las contraseñas no coinciden."));
      return;
    }
    setBusy("identity");
    try {
      const nextIdentity = await invoke<IdentityStatus>("p2p_create_identity", {
        displayName,
        password: newPassword
      });
      setIdentity(nextIdentity);
      setNewPassword("");
      setConfirmPassword("");
      setNotice(t("Identidad P2P creada y desbloqueada para esta sesión."));
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setBusy(null);
    }
  }

  async function unlockIdentity(event: FormEvent) {
    event.preventDefault();
    setError(null);
    setNotice(null);
    setBusy("unlock");
    try {
      const nextIdentity = await invoke<IdentityStatus>("p2p_unlock_identity", {
        password: unlockPassword
      });
      setIdentity(nextIdentity);
      setUnlockPassword("");
      setNotice(t("Identidad P2P desbloqueada."));
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setBusy(null);
    }
  }

  async function lockIdentity() {
    setError(null);
    setNotice(null);
    setBusy("lock");
    try {
      const nextIdentity = await invoke<IdentityStatus>("p2p_lock_identity");
      setIdentity(nextIdentity);
      setNetwork(await invoke<NetworkStatus>("p2p_network_status"));
      setPingResult(null);
      setSearchResults([]);
      setHasSearched(false);
      setNotice(t("Identidad P2P bloqueada."));
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setBusy(null);
    }
  }

  async function startNetwork() {
    setError(null);
    setNotice(null);
    setBusy("network");
    try {
      const status = await invoke<NetworkStatus>("p2p_network_start");
      setNetwork(status);
      setNotice(t("Red P2P iniciada. Ya puedes compartir tu ticket."));
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setBusy(null);
    }
  }

  async function stopNetwork() {
    setError(null);
    setNotice(null);
    setBusy("network");
    try {
      const status = await invoke<NetworkStatus>("p2p_network_stop");
      setNetwork(status);
      setPingResult(null);
      setNotice(t("Red P2P detenida."));
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setBusy(null);
    }
  }

  async function copyOwnTicket() {
    if (!network?.ticket) return;
    setError(null);
    try {
      if (!navigator.clipboard?.writeText) {
        throw new Error(t("El portapapeles no está disponible."));
      }
      await navigator.clipboard.writeText(network.ticket);
      setNotice(t("Ticket de conexión copiado."));
    } catch (cause) {
      setError(errorMessage(cause));
    }
  }

  async function pingRemotePeer(event: FormEvent) {
    event.preventDefault();
    setError(null);
    setNotice(null);
    setBusy("ping");
    try {
      const result = await invoke<PingResult>("p2p_network_ping_ticket", {
        ticket: remoteTicket
      });
      setPingResult(result);
      const nextPeers = await invoke<PeerSummary[]>("p2p_list_peers");
      setPeers(nextPeers);
      setRemotePeer(result.remote_endpoint_id);
      setChatPeer(result.remote_endpoint_id);
      setNotice(t("Conexión autenticada con {name} en {rtt} ms.", {
        name: result.remote_display_name,
        rtt: result.rtt_ms.toFixed(1)
      }));
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setBusy(null);
    }
  }

  async function chooseSharedFolder() {
    const selected = await open({ directory: true, multiple: false });
    if (typeof selected !== "string") return;
    setSelectedPath(selected);
    setShareName(folderName(selected));
  }

  async function addShare(event: FormEvent) {
    event.preventDefault();
    setError(null);
    setNotice(null);
    if (!selectedPath) {
      setError(t("Selecciona una carpeta para compartir."));
      return;
    }
    setBusy("add-share");
    try {
      const share = await invoke<SharedFolder>("p2p_add_share", {
        path: selectedPath,
        name: shareName,
        visibility
      });
      setShares((current) => [...current, share].sort(compareShares));
      setSelectedPath("");
      setShareName("");
      setNotice(t("Carpeta indexada. Ya está lista para publicarse cuando activemos la red."));
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setBusy(null);
    }
  }

  async function reindexShare(shareId: string) {
    await runShareAction(shareId, async () => {
      const updated = await invoke<SharedFolder>("p2p_reindex_share", { shareId });
      replaceShare(updated);
      setNotice(t("Índice de carpeta actualizado."));
    });
  }

  async function toggleShare(share: SharedFolder) {
    await runShareAction(share.id, async () => {
      const updated = await invoke<SharedFolder>("p2p_set_share_enabled", {
        shareId: share.id,
        enabled: !share.enabled
      });
      replaceShare(updated);
      setNotice(updated.enabled ? t("Carpeta habilitada.") : t("Carpeta pausada."));
    });
  }

  async function removeShare(share: SharedFolder) {
    if (!window.confirm(t("¿Dejar de compartir “{name}”? Los archivos originales no se eliminarán.", { name: share.name }))) {
      return;
    }
    await runShareAction(share.id, async () => {
      await invoke("p2p_remove_share", { shareId: share.id });
      setShares((current) => current.filter((item) => item.id !== share.id));
      setSearchResults((current) => current.filter((item) => item.share_id !== share.id));
      setNotice(t("Carpeta quitada del catálogo compartido."));
    });
  }

  async function runShareAction(shareId: string, action: () => Promise<void>) {
    setError(null);
    setNotice(null);
    setBusy(`share:${shareId}`);
    try {
      await action();
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setBusy(null);
    }
  }

  function replaceShare(updated: SharedFolder) {
    setShares((current) => current.map((share) => (share.id === updated.id ? updated : share)));
  }

  async function searchSharedCatalog(event: FormEvent) {
    event.preventDefault();
    setError(null);
    setNotice(null);
    setBusy("search");
    try {
      const response = await invoke<SharedFileSearchResponse>("p2p_search_shared_files", {
        query: searchQuery,
        limit: 100
      });
      setSearchResults(response.results);
      setHasSearched(true);
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setBusy(null);
    }
  }

  async function searchRemoteCatalog(event: FormEvent) {
    event.preventDefault();
    setError(null);
    setNotice(null);
    if (!remotePeer) {
      setError(t("Selecciona un peer con ticket de retorno."));
      return;
    }
    setBusy("remote-search");
    try {
      const response = await invoke<RemoteCatalogResponse>("p2p_remote_search", {
        peerEndpointId: remotePeer,
        query: remoteQuery,
        limit: 100
      });
      setRemoteResults(response.results);
      setHasRemoteSearched(true);
      setNotice(t("Catálogo remoto de {name}: {count} resultado(s).", {
        name: response.peer_display_name,
        count: response.results.length
      }));
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setBusy(null);
    }
  }

  async function downloadRemoteFile(result: SharedFileSearchResult) {
    const destination = await save({ defaultPath: result.name });
    if (typeof destination !== "string") return;
    setError(null);
    setNotice(null);
    setTransfer({
      transfer_id: `pending:${result.file_id}`,
      peer_endpoint_id: remotePeer,
      file_name: result.name,
      destination_path: destination,
      received_bytes: 0,
      total_bytes: result.size_bytes,
      status: "downloading",
      message: t("Iniciando descarga P2P…"),
      occurred_at: new Date().toISOString()
    });
    setBusy(`download:${result.file_id}`);
    try {
      const downloaded = await invoke<DownloadResult>("p2p_download_remote_file", {
        peerEndpointId: remotePeer,
        shareId: result.share_id,
        fileId: result.file_id,
        destinationPath: destination
      });
      setNotice(t("Descarga completada: {name} ({size}).", {
        name: downloaded.name,
        size: formatBytes(downloaded.size_bytes)
      }));
    } catch (cause) {
      setError(errorMessage(cause));
      setTransfer((current) => current ? { ...current, status: "failed", message: errorMessage(cause) } : current);
    } finally {
      setBusy(null);
    }
  }

  async function sendChatMessage(event: FormEvent) {
    event.preventDefault();
    setError(null);
    setNotice(null);
    if (chatRoom === "private" && !chatPeer) {
      setError(t("Selecciona un peer para el chat privado."));
      return;
    }
    setBusy("chat-send");
    try {
      const result = await invoke<ChatSendResult>("p2p_chat_send", {
        room: chatRoom,
        peerEndpointId: chatRoom === "private" ? chatPeer : null,
        body: chatBody
      });
      setChatMessages((current) => upsertChatMessage(current, result.message));
      setChatBody("");
      if (chatRoom === "general") {
        setNotice(t("Mensaje general entregado a {delivered} de {total} peer(s).", {
          delivered: result.delivered_recipients,
          total: result.attempted_recipients
        }));
      }
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setBusy(null);
    }
  }

  if (busy === "loading" && !identity) {
    return (
      <main className="grid min-h-screen place-items-center p-6">
        <LoaderCircle className="h-7 w-7 animate-spin text-muted-foreground" aria-label={t("Cargando")} />
      </main>
    );
  }

  return (
    <main className="min-h-screen bg-background p-4 text-foreground lg:p-6">
      <div className="mx-auto grid w-full max-w-[1480px] gap-4">
        <header className="flex flex-wrap items-start justify-between gap-4">
          <div>
            <div className="flex items-center gap-2 text-muted-foreground">
              <UsersRound className="h-4 w-4" />
              <span className="text-xs font-semibold uppercase tracking-[0.18em]">{t("Rau Connect")}</span>
            </div>
            <h1 className="mt-1 text-2xl font-semibold tracking-tight">{t("Compartir y descubrir")}</h1>
            <p className="mt-1 max-w-3xl text-sm text-muted-foreground">
              {t("Prepara tu identidad y el catálogo que luego viajará directamente entre dispositivos.")}
            </p>
          </div>
          <div className={cn(
            "flex items-center gap-2 rounded-full border px-3 py-1.5 text-xs font-medium",
            network?.running
              ? "border-emerald-500/25 bg-emerald-500/10 text-emerald-800 dark:text-emerald-200"
              : "border-amber-500/25 bg-amber-500/10 text-amber-800 dark:text-amber-200"
          )}>
            <span className={cn("h-2 w-2 rounded-full", network?.running ? "bg-emerald-500" : "bg-amber-500")} />
            {network?.running ? t("Red P2P activa") : t("Red P2P detenida")}
          </div>
        </header>

        {error ? (
          <div role="alert" className="rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive">
            {error}
          </div>
        ) : null}
        {notice ? (
          <div className="rounded-md border border-emerald-500/25 bg-emerald-500/10 px-3 py-2 text-sm text-emerald-800 dark:text-emerald-200">
            {notice}
          </div>
        ) : null}

        {!identity?.configured ? (
          <IdentitySetup
            displayName={displayName}
            password={newPassword}
            confirmation={confirmPassword}
            busy={busy === "identity"}
            onDisplayName={setDisplayName}
            onPassword={setNewPassword}
            onConfirmation={setConfirmPassword}
            onSubmit={createIdentity}
          />
        ) : !identity.unlocked ? (
          <IdentityUnlock
            identity={identity}
            password={unlockPassword}
            busy={busy === "unlock"}
            onPassword={setUnlockPassword}
            onSubmit={unlockIdentity}
          />
        ) : (
          <>
            <section className="grid gap-3 md:grid-cols-3">
              <MetricCard
                icon={<ShieldCheck className="h-4 w-4" />}
                label={t("Identidad")}
                value={identity.display_name ?? t("Configurada")}
                detail={shortEndpoint(identity.endpoint_id)}
              />
              <MetricCard
                icon={<UsersRound className="h-4 w-4" />}
                label={t("Contactos")}
                value={String(peers.length)}
                detail={peers.some((peer) => peer.presence_status === "online") ? t("Hay contactos conectados") : t("Sin contactos conectados")}
              />
              <MetricCard
                icon={<HardDrive className="h-4 w-4" />}
                label={t("Catálogo compartido")}
                value={String(sharedFileCount)}
                detail={t("{count} carpeta(s)", { count: shares.length })}
              />
            </section>

            <NetworkPanel
              network={network}
              peers={peers}
              events={networkEvents}
              remoteTicket={remoteTicket}
              pingResult={pingResult}
              busy={busy}
              onRemoteTicket={setRemoteTicket}
              onStart={() => void startNetwork()}
              onStop={() => void stopNetwork()}
              onCopyTicket={() => void copyOwnTicket()}
              onPing={pingRemotePeer}
            />

            <section className="grid items-start gap-4 xl:grid-cols-2">
              <RemoteCatalogPanel
                peers={reachablePeers}
                selectedPeer={remotePeer}
                query={remoteQuery}
                results={remoteResults}
                searched={hasRemoteSearched}
                transfer={transfer}
                busy={busy}
                networkRunning={network?.running === true}
                onPeer={(endpointId) => {
                  setRemotePeer(endpointId);
                  setRemoteResults([]);
                  setHasRemoteSearched(false);
                }}
                onQuery={setRemoteQuery}
                onSearch={searchRemoteCatalog}
                onDownload={(result) => void downloadRemoteFile(result)}
              />
              <ChatPanel
                identity={identity}
                peers={reachablePeers}
                room={chatRoom}
                selectedPeer={chatPeer}
                messages={chatMessages}
                body={chatBody}
                busy={busy === "chat-send"}
                networkRunning={network?.running === true}
                onRoom={setChatRoom}
                onPeer={setChatPeer}
                onBody={setChatBody}
                onSend={sendChatMessage}
              />
            </section>

            <Card>
              <CardHeader>
                <CardTitle className="flex items-center gap-2">
                  <KeyRound className="h-4 w-4" />
                  {t("Identidad del dispositivo")}
                </CardTitle>
                <Button variant="ghost" size="sm" disabled={busy === "lock"} onClick={() => void lockIdentity()}>
                  {busy === "lock" ? <LoaderCircle className="h-4 w-4 animate-spin" /> : <LockKeyhole className="h-4 w-4" />}
                  {t("Bloquear")}
                </Button>
              </CardHeader>
              <CardContent className="grid gap-2 p-3 text-sm sm:grid-cols-[180px_minmax(0,1fr)]">
                <span className="text-muted-foreground">{t("Nombre público")}</span>
                <strong>{identity.display_name}</strong>
                <span className="text-muted-foreground">Endpoint ID</span>
                <code className="break-all rounded bg-secondary px-2 py-1 text-xs">{identity.endpoint_id}</code>
              </CardContent>
            </Card>

            <section className="grid items-start gap-4 xl:grid-cols-[minmax(330px,.8fr)_minmax(0,1.2fr)]">
              <ShareFolderForm
                selectedPath={selectedPath}
                shareName={shareName}
                visibility={visibility}
                busy={busy === "add-share"}
                onChoose={() => void chooseSharedFolder()}
                onName={setShareName}
                onVisibility={setVisibility}
                onSubmit={addShare}
              />
              <SharedFolderList
                shares={shares}
                busy={busy}
                onReindex={(shareId) => void reindexShare(shareId)}
                onToggle={(share) => void toggleShare(share)}
                onRemove={(share) => void removeShare(share)}
              />
            </section>

            <SharedCatalogSearch
              query={searchQuery}
              results={searchResults}
              searched={hasSearched}
              busy={busy === "search"}
              onQuery={setSearchQuery}
              onSubmit={searchSharedCatalog}
            />
          </>
        )}
      </div>
    </main>
  );
}

function RemoteCatalogPanel({
  peers,
  selectedPeer,
  query,
  results,
  searched,
  transfer,
  busy,
  networkRunning,
  onPeer,
  onQuery,
  onSearch,
  onDownload
}: {
  peers: PeerSummary[];
  selectedPeer: string;
  query: string;
  results: SharedFileSearchResult[];
  searched: boolean;
  transfer: TransferEvent | null;
  busy: BusyAction;
  networkRunning: boolean;
  onPeer: (endpointId: string) => void;
  onQuery: (query: string) => void;
  onSearch: (event: FormEvent) => void;
  onDownload: (result: SharedFileSearchResult) => void;
}) {
  const { t } = useI18n();
  const searchBusy = busy === "remote-search";
  const progress = transfer && transfer.total_bytes > 0
    ? Math.min(100, (transfer.received_bytes / transfer.total_bytes) * 100)
    : 0;

  return (
    <Card>
      <CardHeader>
        <div>
          <CardTitle className="flex items-center gap-2">
            <Library className="h-4 w-4" />
            {t("Biblioteca remota")}
          </CardTitle>
          <p className="mt-0.5 text-xs text-muted-foreground">
            {t("Busca metadata remota y descarga el archivo directamente desde el peer.")}
          </p>
        </div>
      </CardHeader>
      <CardContent className="grid gap-3 p-3">
        <form className="grid gap-2" onSubmit={onSearch}>
          <Field label={t("Dispositivo remoto")}>
            <select
              className={fieldClass}
              value={selectedPeer}
              disabled={!networkRunning || peers.length === 0}
              onChange={(event) => onPeer(event.target.value)}
            >
              {peers.length === 0 ? <option value="">{t("Sin peers con ticket de retorno")}</option> : null}
              {peers.map((peer) => (
                <option key={peer.endpoint_id} value={peer.endpoint_id}>
                  {peer.display_name} · {shortEndpoint(peer.endpoint_id)}
                </option>
              ))}
            </select>
          </Field>
          <div className="grid gap-2 sm:grid-cols-[minmax(0,1fr)_auto]">
            <input
              className={fieldClass}
              value={query}
              disabled={!networkRunning || !selectedPeer}
              placeholder={t("Buscar en los archivos del peer…")}
              onChange={(event) => onQuery(event.target.value)}
            />
            <Button disabled={!networkRunning || !selectedPeer || searchBusy}>
              {searchBusy ? <LoaderCircle className="h-4 w-4 animate-spin" /> : <Search className="h-4 w-4" />}
              {t("Buscar remoto")}
            </Button>
          </div>
        </form>

        {peers.length === 0 ? (
          <div className="rounded-md border border-dashed border-border p-4 text-center text-xs leading-5 text-muted-foreground">
            {t("Vuelve a probar la conexión para intercambiar tickets de retorno con el otro dispositivo.")}
          </div>
        ) : null}

        {transfer ? (
          <div className={cn(
            "rounded-md border p-3",
            transfer.status === "failed" ? "border-destructive/30 bg-destructive/10" : "border-border bg-secondary/45"
          )}>
            <div className="flex items-center justify-between gap-3 text-xs">
              <strong className="truncate">{transfer.file_name}</strong>
              <span className="shrink-0 text-muted-foreground">
                {formatBytes(transfer.received_bytes)} / {formatBytes(transfer.total_bytes)}
              </span>
            </div>
            <div className="mt-2 h-1.5 overflow-hidden rounded-full bg-background">
              <div
                className={cn("h-full transition-[width]", transfer.status === "failed" ? "bg-destructive" : "bg-emerald-500")}
                style={{ width: `${progress}%` }}
              />
            </div>
            <p className="mt-1.5 truncate text-[10px] text-muted-foreground">{transfer.message}</p>
          </div>
        ) : null}

        {!searched ? (
          <div className="rounded-md border border-dashed border-border p-5 text-center text-sm text-muted-foreground">
            {t("Busca sin texto para listar los primeros archivos que el peer autoriza.")}
          </div>
        ) : results.length === 0 ? (
          <div className="rounded-md border border-dashed border-border p-5 text-center text-sm text-muted-foreground">
            {t("El peer no devolvió archivos para esta búsqueda.")}
          </div>
        ) : (
          <div className="overflow-hidden rounded-md border border-border">
            <div className="grid grid-cols-[minmax(0,1fr)_100px_80px_36px] gap-2 border-b border-border bg-secondary/50 px-3 py-2 text-[10px] font-semibold uppercase tracking-wide text-muted-foreground max-sm:grid-cols-[minmax(0,1fr)_36px]">
              <span>{t("Archivo")}</span>
              <span className="max-sm:hidden">{t("Carpeta")}</span>
              <span className="max-sm:hidden">{t("Tamaño")}</span>
              <span />
            </div>
            <div className="max-h-80 overflow-y-auto">
              {results.map((result) => {
                const downloading = busy === `download:${result.file_id}`;
                return (
                  <div key={`${result.share_id}:${result.file_id}`} className="grid grid-cols-[minmax(0,1fr)_100px_80px_36px] items-center gap-2 border-b border-border px-3 py-2 last:border-b-0 max-sm:grid-cols-[minmax(0,1fr)_36px]">
                    <div className="min-w-0">
                      <strong className="block truncate text-xs">{result.name}</strong>
                      <span className="block truncate text-[10px] text-muted-foreground">{result.relative_path}</span>
                    </div>
                    <span className="truncate text-[10px] text-muted-foreground max-sm:hidden">{result.share_name}</span>
                    <span className="text-[10px] text-muted-foreground max-sm:hidden">{formatBytes(result.size_bytes)}</span>
                    <Button
                      type="button"
                      size="icon"
                      variant="ghost"
                      disabled={!networkRunning || downloading}
                      aria-label={t("Descargar {name}", { name: result.name })}
                      title={t("Descargar")}
                      onClick={() => onDownload(result)}
                    >
                      {downloading ? <LoaderCircle className="h-4 w-4 animate-spin" /> : <Download className="h-4 w-4" />}
                    </Button>
                  </div>
                );
              })}
            </div>
          </div>
        )}
      </CardContent>
    </Card>
  );
}

function ChatPanel({
  identity,
  peers,
  room,
  selectedPeer,
  messages,
  body,
  busy,
  networkRunning,
  onRoom,
  onPeer,
  onBody,
  onSend
}: {
  identity: IdentityStatus;
  peers: PeerSummary[];
  room: ChatRoom;
  selectedPeer: string;
  messages: ChatMessage[];
  body: string;
  busy: boolean;
  networkRunning: boolean;
  onRoom: (room: ChatRoom) => void;
  onPeer: (endpointId: string) => void;
  onBody: (body: string) => void;
  onSend: (event: FormEvent) => void;
}) {
  const { t } = useI18n();
  const canSend = networkRunning && peers.length > 0 && (room === "general" || Boolean(selectedPeer));

  return (
    <Card>
      <CardHeader>
        <div>
          <CardTitle className="flex items-center gap-2">
            <MessageCircle className="h-4 w-4" />
            {t("Chat P2P")}
          </CardTitle>
          <p className="mt-0.5 text-xs text-muted-foreground">
            {t("Los mensajes viajan por Iroh y se guardan localmente en SQLite.")}
          </p>
        </div>
        <div className="flex rounded-md border border-border p-0.5">
          <Button type="button" size="sm" variant={room === "private" ? "secondary" : "ghost"} onClick={() => onRoom("private")}>
            {t("Privado")}
          </Button>
          <Button type="button" size="sm" variant={room === "general" ? "secondary" : "ghost"} onClick={() => onRoom("general")}>
            <Globe2 className="h-3.5 w-3.5" />
            {t("General")}
          </Button>
        </div>
      </CardHeader>
      <CardContent className="grid gap-3 p-3">
        {room === "private" ? (
          <select
            className={fieldClass}
            value={selectedPeer}
            disabled={!networkRunning || peers.length === 0}
            aria-label={t("Destinatario del chat privado")}
            onChange={(event) => onPeer(event.target.value)}
          >
            {peers.length === 0 ? <option value="">{t("Sin peers con ticket de retorno")}</option> : null}
            {peers.map((peer) => <option key={peer.endpoint_id} value={peer.endpoint_id}>{peer.display_name}</option>)}
          </select>
        ) : (
          <p className="rounded-md bg-secondary/50 px-3 py-2 text-xs leading-5 text-muted-foreground">
            {t("El chat general se difunde a todos tus peers conocidos; todavía no es una sala pública global.")}
          </p>
        )}

        <div className="grid min-h-72 max-h-96 content-start gap-2 overflow-y-auto rounded-md border border-border bg-background p-3">
          {messages.length === 0 ? (
            <div className="grid min-h-64 place-items-center text-center text-sm text-muted-foreground">
              {t("Todavía no hay mensajes en esta conversación.")}
            </div>
          ) : messages.map((message) => {
            const outgoing = message.direction === "outgoing" || message.sender_endpoint_id === identity.endpoint_id;
            return (
              <article key={message.id} className={cn("flex", outgoing ? "justify-end" : "justify-start")}>
                <div className={cn(
                  "max-w-[86%] rounded-lg px-3 py-2",
                  outgoing ? "bg-primary text-primary-foreground" : "bg-secondary text-secondary-foreground"
                )}>
                  {!outgoing || room === "general" ? (
                    <strong className="mb-1 block text-[10px] opacity-75">{message.sender_display_name}</strong>
                  ) : null}
                  <p className="whitespace-pre-wrap break-words text-sm leading-5">{message.body}</p>
                  <div className="mt-1 flex items-center justify-end gap-2 text-[9px] opacity-65">
                    <time>{formatChatTime(message.sent_at)}</time>
                    {outgoing ? <span>{deliveryLabel(message.delivery_status, t)}</span> : null}
                  </div>
                </div>
              </article>
            );
          })}
        </div>

        <form className="grid gap-2" onSubmit={onSend}>
          <textarea
            className="min-h-20 w-full resize-y rounded-md border border-border bg-background p-3 text-sm text-foreground outline-none focus:border-foreground/35 focus:ring-2 focus:ring-ring/30 disabled:cursor-not-allowed disabled:opacity-60"
            value={body}
            maxLength={4000}
            disabled={!canSend || busy}
            required
            placeholder={room === "private" ? t("Escribe un mensaje privado…") : t("Escribe para tus peers conocidos…")}
            onChange={(event) => onBody(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter" && !event.shiftKey && canSend && body.trim()) {
                event.preventDefault();
                event.currentTarget.form?.requestSubmit();
              }
            }}
          />
          <div className="flex items-center justify-between gap-3">
            <span className="text-[10px] text-muted-foreground">{body.length}/4000</span>
            <Button disabled={!canSend || busy || !body.trim()}>
              {busy ? <LoaderCircle className="h-4 w-4 animate-spin" /> : <Send className="h-4 w-4" />}
              {t("Enviar mensaje")}
            </Button>
          </div>
        </form>
      </CardContent>
    </Card>
  );
}

function NetworkPanel({
  network,
  peers,
  events,
  remoteTicket,
  pingResult,
  busy,
  onRemoteTicket,
  onStart,
  onStop,
  onCopyTicket,
  onPing
}: {
  network: NetworkStatus | null;
  peers: PeerSummary[];
  events: NetworkEvent[];
  remoteTicket: string;
  pingResult: PingResult | null;
  busy: BusyAction;
  onRemoteTicket: (value: string) => void;
  onStart: () => void;
  onStop: () => void;
  onCopyTicket: () => void;
  onPing: (event: FormEvent) => void;
}) {
  const { t } = useI18n();
  const running = network?.running === true;
  const networkBusy = busy === "network";
  const pingBusy = busy === "ping";

  return (
    <Card>
      <CardHeader>
        <div>
          <CardTitle className="flex items-center gap-2">
            {running ? <Wifi className="h-4 w-4 text-emerald-500" /> : <WifiOff className="h-4 w-4 text-muted-foreground" />}
            {t("Tráfico P2P")}
          </CardTitle>
          <p className="mt-0.5 text-xs text-muted-foreground">
            {t("Conexiones Iroh autenticadas, directas cuando es posible y con relay como respaldo.")}
          </p>
        </div>
        <Button
          size="sm"
          variant={running ? "secondary" : "default"}
          disabled={networkBusy}
          onClick={running ? onStop : onStart}
        >
          {networkBusy ? (
            <LoaderCircle className="h-4 w-4 animate-spin" />
          ) : running ? (
            <WifiOff className="h-4 w-4" />
          ) : (
            <Wifi className="h-4 w-4" />
          )}
          {running ? t("Detener red") : t("Iniciar red")}
        </Button>
      </CardHeader>
      <CardContent className="grid gap-4 p-3 xl:grid-cols-[minmax(0,1.25fr)_minmax(300px,.75fr)]">
        <div className="grid content-start gap-4">
          <div className="flex flex-wrap gap-2 text-xs">
            <StatusPill active={running} icon={<Radio className="h-3.5 w-3.5" />}>
              {running ? t("Endpoint activo") : t("Endpoint detenido")}
            </StatusPill>
            <StatusPill active={network?.relay_ready === true} icon={<Network className="h-3.5 w-3.5" />}>
              {network?.relay_ready ? t("Relay disponible") : t("Esperando relay")}
            </StatusPill>
            <StatusPill active={(network?.address_count ?? 0) > 0} icon={<Activity className="h-3.5 w-3.5" />}>
              {t("{count} dirección(es)", { count: network?.address_count ?? 0 })}
            </StatusPill>
          </div>

          {running ? (
            <div className="grid gap-2">
              <div className="flex items-end justify-between gap-3">
                <div>
                  <h3 className="text-xs font-semibold">{t("Mi ticket de conexión")}</h3>
                  <p className="mt-0.5 text-xs text-muted-foreground">
                    {t("Puedes enviarlo como texto; el QR de emparejamiento contendrá este mismo ticket.")}
                  </p>
                </div>
                <Button type="button" size="sm" variant="secondary" disabled={!network?.ticket} onClick={onCopyTicket}>
                  <Copy className="h-4 w-4" />
                  {t("Copiar")}
                </Button>
              </div>
              <textarea
                className="min-h-24 w-full resize-y rounded-md border border-border bg-secondary/45 p-2 font-mono text-[11px] leading-5 text-foreground outline-none focus:border-foreground/35 focus:ring-2 focus:ring-ring/30"
                value={network?.ticket ?? ""}
                readOnly
                aria-label={t("Mi ticket de conexión")}
              />
              <code className="break-all text-[10px] text-muted-foreground">{network?.endpoint_id}</code>
            </div>
          ) : (
            <div className="rounded-md border border-dashed border-border p-5 text-sm leading-6 text-muted-foreground">
              {t("Inicia la red para generar un ticket alcanzable y aceptar conexiones de otros dispositivos.")}
            </div>
          )}

          <form className="grid gap-2" onSubmit={onPing}>
            <div>
              <h3 className="text-xs font-semibold">{t("Conectar otro dispositivo")}</h3>
              <p className="mt-0.5 text-xs text-muted-foreground">
                {t("Pega su ticket para comprobar identidad, ruta de red y latencia real.")}
              </p>
            </div>
            <textarea
              className="min-h-20 w-full resize-y rounded-md border border-border bg-background p-2 font-mono text-[11px] leading-5 text-foreground outline-none focus:border-foreground/35 focus:ring-2 focus:ring-ring/30 disabled:cursor-not-allowed disabled:opacity-60"
              value={remoteTicket}
              disabled={!running || pingBusy}
              required
              placeholder={t("Pega aquí el ticket Iroh del otro dispositivo…")}
              onChange={(event) => onRemoteTicket(event.target.value)}
            />
            <div className="flex flex-wrap items-center gap-3">
              <Button className="justify-self-start" disabled={!running || pingBusy || !remoteTicket.trim()}>
                {pingBusy ? <LoaderCircle className="h-4 w-4 animate-spin" /> : <Send className="h-4 w-4" />}
                {pingBusy ? t("Conectando…") : t("Probar conexión")}
              </Button>
              {pingResult ? (
                <span className="text-xs text-emerald-700 dark:text-emerald-300">
                  {t("{name} respondió en {rtt} ms", {
                    name: pingResult.remote_display_name,
                    rtt: pingResult.rtt_ms.toFixed(1)
                  })}
                </span>
              ) : null}
            </div>
          </form>
        </div>

        <div className="grid content-start gap-4">
          <div>
            <div className="mb-2 flex items-center justify-between gap-2">
              <h3 className="flex items-center gap-2 text-xs font-semibold">
                <UsersRound className="h-4 w-4" />
                {t("Dispositivos conocidos")}
              </h3>
              <span className="text-xs text-muted-foreground">{peers.length}</span>
            </div>
            {peers.length === 0 ? (
              <div className="rounded-md border border-dashed border-border p-4 text-center text-xs leading-5 text-muted-foreground">
                {t("Los dispositivos aparecerán aquí después de la primera conexión autenticada.")}
              </div>
            ) : (
              <div className="grid gap-2">
                {peers.map((peer) => {
                  const online = peer.presence_status === "online";
                  return (
                    <article key={peer.endpoint_id} className="rounded-md border border-border p-2.5">
                      <div className="flex items-start justify-between gap-3">
                        <div className="min-w-0">
                          <strong className="block truncate text-xs">{peer.display_name}</strong>
                          <code className="mt-0.5 block truncate text-[10px] text-muted-foreground">{peer.endpoint_id}</code>
                        </div>
                        <span className={cn(
                          "inline-flex shrink-0 items-center gap-1.5 rounded-full px-2 py-0.5 text-[10px] font-medium",
                          online
                            ? "bg-emerald-500/10 text-emerald-700 dark:text-emerald-300"
                            : "bg-secondary text-muted-foreground"
                        )}>
                          <span className={cn("h-1.5 w-1.5 rounded-full", online ? "bg-emerald-500" : "bg-muted-foreground")} />
                          {online ? t("Conectado") : t("Offline")}
                        </span>
                      </div>
                      <p className="mt-2 text-[10px] text-muted-foreground">
                        {peer.last_seen_at
                          ? t("Última actividad: {date}", { date: formatDate(peer.last_seen_at) })
                          : t("Sin actividad registrada")}
                      </p>
                      <p className={cn("mt-1 text-[10px]", peer.can_connect ? "text-emerald-700 dark:text-emerald-300" : "text-amber-700 dark:text-amber-300")}>
                        {peer.can_connect
                          ? t("Catálogo y chat disponibles")
                          : t("Repite la conexión para habilitar catálogo y chat")}
                      </p>
                    </article>
                  );
                })}
              </div>
            )}
          </div>

          {events.length > 0 ? (
            <div>
              <h3 className="mb-2 flex items-center gap-2 text-xs font-semibold">
                <Activity className="h-4 w-4" />
                {t("Actividad de red")}
              </h3>
              <div className="grid gap-1.5">
                {events.map((event, index) => (
                  <div key={`${event.kind}:${event.occurred_at}:${index}`} className="rounded-md bg-secondary/55 px-2.5 py-2">
                    <p className="text-[11px]">{event.message}</p>
                    <time className="text-[10px] text-muted-foreground">{formatDate(event.occurred_at)}</time>
                  </div>
                ))}
              </div>
            </div>
          ) : null}
        </div>
      </CardContent>
    </Card>
  );
}

function StatusPill({ active, icon, children }: { active: boolean; icon: ReactNode; children: ReactNode }) {
  return (
    <span className={cn(
      "inline-flex items-center gap-1.5 rounded-full border px-2.5 py-1",
      active
        ? "border-emerald-500/25 bg-emerald-500/10 text-emerald-800 dark:text-emerald-200"
        : "border-border bg-secondary/60 text-muted-foreground"
    )}>
      {icon}
      {children}
    </span>
  );
}

function IdentitySetup({
  displayName,
  password,
  confirmation,
  busy,
  onDisplayName,
  onPassword,
  onConfirmation,
  onSubmit
}: {
  displayName: string;
  password: string;
  confirmation: string;
  busy: boolean;
  onDisplayName: (value: string) => void;
  onPassword: (value: string) => void;
  onConfirmation: (value: string) => void;
  onSubmit: (event: FormEvent) => void;
}) {
  const { t } = useI18n();
  return (
    <Card className="mx-auto w-full max-w-2xl">
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <KeyRound className="h-4 w-4" />
          {t("Crear identidad P2P")}
        </CardTitle>
      </CardHeader>
      <CardContent className="p-4">
        <p className="mb-4 text-sm leading-6 text-muted-foreground">
          {t("La clave privada se cifra con tu contraseña y se guarda dentro de SQLite. Rau no puede recuperar una contraseña olvidada.")}
        </p>
        <form className="grid gap-3" onSubmit={onSubmit}>
          <Field label={t("Nombre público")}>
            <input className={fieldClass} value={displayName} minLength={2} maxLength={64} required autoFocus onChange={(event) => onDisplayName(event.target.value)} />
          </Field>
          <div className="grid gap-3 sm:grid-cols-2">
            <Field label={t("Contraseña")} hint={t("Mínimo 10 caracteres")}>
              <input className={fieldClass} type="password" value={password} minLength={10} required autoComplete="new-password" onChange={(event) => onPassword(event.target.value)} />
            </Field>
            <Field label={t("Confirmar contraseña")}>
              <input className={fieldClass} type="password" value={confirmation} minLength={10} required autoComplete="new-password" onChange={(event) => onConfirmation(event.target.value)} />
            </Field>
          </div>
          <Button className="mt-1 justify-self-start" disabled={busy}>
            {busy ? <LoaderCircle className="h-4 w-4 animate-spin" /> : <ShieldCheck className="h-4 w-4" />}
            {t("Crear identidad")}
          </Button>
        </form>
      </CardContent>
    </Card>
  );
}

function IdentityUnlock({
  identity,
  password,
  busy,
  onPassword,
  onSubmit
}: {
  identity: IdentityStatus;
  password: string;
  busy: boolean;
  onPassword: (value: string) => void;
  onSubmit: (event: FormEvent) => void;
}) {
  const { t } = useI18n();
  return (
    <Card className="mx-auto w-full max-w-xl">
      <CardHeader>
        <CardTitle className="flex items-center gap-2"><LockKeyhole className="h-4 w-4" />{t("Desbloquear identidad")}</CardTitle>
      </CardHeader>
      <CardContent className="p-4">
        <p className="mb-4 text-sm text-muted-foreground">
          {t("La identidad de {name} está cifrada en este dispositivo.", { name: identity.display_name })}
        </p>
        <form className="grid gap-3" onSubmit={onSubmit}>
          <Field label={t("Contraseña")}>
            <input className={fieldClass} type="password" value={password} required autoFocus autoComplete="current-password" onChange={(event) => onPassword(event.target.value)} />
          </Field>
          <Button className="justify-self-start" disabled={busy}>
            {busy ? <LoaderCircle className="h-4 w-4 animate-spin" /> : <KeyRound className="h-4 w-4" />}
            {t("Desbloquear")}
          </Button>
        </form>
      </CardContent>
    </Card>
  );
}

function ShareFolderForm({
  selectedPath,
  shareName,
  visibility,
  busy,
  onChoose,
  onName,
  onVisibility,
  onSubmit
}: {
  selectedPath: string;
  shareName: string;
  visibility: SharedFolder["visibility"];
  busy: boolean;
  onChoose: () => void;
  onName: (value: string) => void;
  onVisibility: (value: SharedFolder["visibility"]) => void;
  onSubmit: (event: FormEvent) => void;
}) {
  const { t } = useI18n();
  return (
    <Card>
      <CardHeader><CardTitle className="flex items-center gap-2"><FolderOpen className="h-4 w-4" />{t("Compartir una carpeta")}</CardTitle></CardHeader>
      <CardContent className="p-3">
        <form className="grid gap-3" onSubmit={onSubmit}>
          <Field label={t("Carpeta local")}>
            <div className="grid gap-2 sm:grid-cols-[minmax(0,1fr)_auto]">
              <input className={fieldClass} value={selectedPath} readOnly placeholder={t("Ninguna carpeta seleccionada")} />
              <Button type="button" variant="secondary" onClick={onChoose}><FolderOpen className="h-4 w-4" />{t("Elegir")}</Button>
            </div>
          </Field>
          <Field label={t("Nombre visible")}>
            <input className={fieldClass} value={shareName} maxLength={80} required disabled={!selectedPath} onChange={(event) => onName(event.target.value)} />
          </Field>
          <Field label={t("Visibilidad")}>
            <select className={fieldClass} value={visibility} onChange={(event) => onVisibility(event.target.value as SharedFolder["visibility"])}>
              <option value="contacts">{t("Todos mis contactos")}</option>
              <option value="selected_contacts">{t("Contactos seleccionados")}</option>
              <option value="community">{t("Comunidad general")}</option>
              <option value="ticket">{t("Solo mediante invitación")}</option>
            </select>
          </Field>
          <p className="text-xs leading-5 text-muted-foreground">
            {t("Solo se publica una ruta virtual. Las rutas absolutas y los archivos ocultos no entran al catálogo.")}
          </p>
          <Button className="justify-self-start" disabled={busy || !selectedPath}>
            {busy ? <LoaderCircle className="h-4 w-4 animate-spin" /> : <HardDrive className="h-4 w-4" />}
            {t("Indexar carpeta")}
          </Button>
        </form>
      </CardContent>
    </Card>
  );
}

function SharedFolderList({
  shares,
  busy,
  onReindex,
  onToggle,
  onRemove
}: {
  shares: SharedFolder[];
  busy: BusyAction;
  onReindex: (shareId: string) => void;
  onToggle: (share: SharedFolder) => void;
  onRemove: (share: SharedFolder) => void;
}) {
  const { t } = useI18n();
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("Carpetas compartidas")}</CardTitle>
        <span className="text-xs text-muted-foreground">{shares.length}</span>
      </CardHeader>
      <CardContent className="p-3">
        {shares.length === 0 ? (
          <div className="grid min-h-44 place-items-center rounded-md border border-dashed border-border p-6 text-center text-sm text-muted-foreground">
            {t("Todavía no has preparado carpetas para compartir.")}
          </div>
        ) : (
          <div className="grid gap-2">
            {shares.map((share) => {
              const shareBusy = busy === `share:${share.id}`;
              return (
                <article key={share.id} className={cn("rounded-md border border-border p-3", !share.enabled && "opacity-65")}>
                  <div className="flex flex-wrap items-start justify-between gap-3">
                    <div className="min-w-0">
                      <div className="flex items-center gap-2">
                        <span className={cn("h-2 w-2 rounded-full", share.enabled ? "bg-emerald-500" : "bg-muted-foreground")} />
                        <strong className="truncate text-sm">{share.name}</strong>
                        <VisibilityBadge visibility={share.visibility} />
                      </div>
                      <p className="mt-1 truncate text-xs text-muted-foreground" title={share.root_path}>{share.root_path}</p>
                    </div>
                    <div className="flex items-center gap-1">
                      <Button size="icon" variant="ghost" disabled={shareBusy} aria-label={t("Reindexar")} title={t("Reindexar")} onClick={() => onReindex(share.id)}>
                        {shareBusy ? <LoaderCircle className="h-4 w-4 animate-spin" /> : <RefreshCcw className="h-4 w-4" />}
                      </Button>
                      <Button size="icon" variant="ghost" disabled={shareBusy} aria-label={share.enabled ? t("Pausar") : t("Habilitar")} title={share.enabled ? t("Pausar") : t("Habilitar")} onClick={() => onToggle(share)}>
                        {share.enabled ? <Pause className="h-4 w-4" /> : <Play className="h-4 w-4" />}
                      </Button>
                      <Button size="icon" variant="ghost" disabled={shareBusy} aria-label={t("Eliminar")} title={t("Eliminar")} onClick={() => onRemove(share)}>
                        <Trash2 className="h-4 w-4 text-destructive" />
                      </Button>
                    </div>
                  </div>
                  <div className="mt-3 flex flex-wrap gap-x-4 gap-y-1 text-xs text-muted-foreground">
                    <span>{t("{count} archivo(s)", { count: share.file_count })}</span>
                    <span>{formatBytes(share.total_size_bytes)}</span>
                    <span>{t("Indexada {date}", { date: formatDate(share.last_indexed_at) })}</span>
                    {share.skipped_entries > 0 ? <span className="text-amber-700 dark:text-amber-300">{t("{count} omitidos", { count: share.skipped_entries })}</span> : null}
                  </div>
                </article>
              );
            })}
          </div>
        )}
      </CardContent>
    </Card>
  );
}

function SharedCatalogSearch({
  query,
  results,
  searched,
  busy,
  onQuery,
  onSubmit
}: {
  query: string;
  results: SharedFileSearchResult[];
  searched: boolean;
  busy: boolean;
  onQuery: (value: string) => void;
  onSubmit: (event: FormEvent) => void;
}) {
  const { t } = useI18n();
  return (
    <Card>
      <CardHeader>
        <div>
          <CardTitle className="flex items-center gap-2"><Search className="h-4 w-4" />{t("Vista previa del catálogo")}</CardTitle>
          <p className="mt-0.5 text-xs text-muted-foreground">{t("Valida ahora los resultados que recibiría un peer remoto.")}</p>
        </div>
      </CardHeader>
      <CardContent className="p-3">
        <form className="mb-3 grid gap-2 sm:grid-cols-[minmax(0,1fr)_auto]" onSubmit={onSubmit}>
          <input className={fieldClass} value={query} placeholder={t("Buscar por nombre, carpeta o extensión…")} onChange={(event) => onQuery(event.target.value)} />
          <Button disabled={busy}>
            {busy ? <LoaderCircle className="h-4 w-4 animate-spin" /> : <Search className="h-4 w-4" />}
            {t("Buscar")}
          </Button>
        </form>

        {!searched ? (
          <div className="rounded-md border border-dashed border-border p-5 text-center text-sm text-muted-foreground">
            {t("Busca sin texto para revisar hasta 100 archivos del catálogo habilitado.")}
          </div>
        ) : results.length === 0 ? (
          <div className="rounded-md border border-dashed border-border p-5 text-center text-sm text-muted-foreground">{t("No se encontraron archivos compartidos.")}</div>
        ) : (
          <div className="overflow-hidden rounded-md border border-border">
            <div className="grid grid-cols-[minmax(0,1fr)_110px_110px] gap-3 border-b border-border bg-secondary/50 px-3 py-2 text-[11px] font-semibold uppercase tracking-wide text-muted-foreground max-sm:grid-cols-[minmax(0,1fr)_90px]">
              <span>{t("Archivo")}</span><span>{t("Carpeta")}</span><span className="max-sm:hidden">{t("Tamaño")}</span>
            </div>
            <div className="max-h-96 overflow-y-auto">
              {results.map((result) => (
                <div key={`${result.share_id}:${result.file_id}`} className="grid grid-cols-[minmax(0,1fr)_110px_110px] items-center gap-3 border-b border-border px-3 py-2 text-sm last:border-b-0 max-sm:grid-cols-[minmax(0,1fr)_90px]">
                  <div className="min-w-0">
                    <strong className="block truncate text-xs">{result.name}</strong>
                    <span className="block truncate text-[11px] text-muted-foreground">{result.relative_path}</span>
                  </div>
                  <span className="truncate text-xs text-muted-foreground">{result.share_name}</span>
                  <span className="text-xs text-muted-foreground max-sm:hidden">{formatBytes(result.size_bytes)}</span>
                </div>
              ))}
            </div>
          </div>
        )}
      </CardContent>
    </Card>
  );
}

function MetricCard({ icon, label, value, detail }: { icon: ReactNode; label: string; value: string; detail: string }) {
  return (
    <Card className="p-3">
      <div className="flex items-center gap-2 text-xs font-medium text-muted-foreground">{icon}{label}</div>
      <strong className="mt-2 block truncate text-xl">{value}</strong>
      <span className="mt-0.5 block truncate text-xs text-muted-foreground">{detail}</span>
    </Card>
  );
}

function Field({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) {
  return (
    <label className="grid gap-1.5 text-xs font-medium">
      <span className="flex items-center justify-between gap-2"><span>{label}</span>{hint ? <span className="font-normal text-muted-foreground">{hint}</span> : null}</span>
      {children}
    </label>
  );
}

function VisibilityBadge({ visibility }: { visibility: SharedFolder["visibility"] }) {
  const { t } = useI18n();
  const labels: Record<SharedFolder["visibility"], string> = {
    contacts: t("Contactos"),
    selected_contacts: t("Seleccionados"),
    community: t("Comunidad"),
    ticket: t("Invitación")
  };
  return <span className="rounded-full border border-border bg-secondary px-2 py-0.5 text-[10px] font-medium text-muted-foreground">{labels[visibility]}</span>;
}

function compareShares(left: SharedFolder, right: SharedFolder) {
  return left.name.localeCompare(right.name, undefined, { sensitivity: "base" });
}

function shortEndpoint(value?: string | null) {
  if (!value) return "—";
  return value.length > 22 ? `${value.slice(0, 11)}…${value.slice(-8)}` : value;
}

function folderName(path: string) {
  const normalized = path.replace(/[\\/]+$/, "");
  return normalized.split(/[\\/]/).pop() || "Shared folder";
}

function formatBytes(value: number) {
  if (!Number.isFinite(value) || value <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const index = Math.min(Math.floor(Math.log(value) / Math.log(1024)), units.length - 1);
  const amount = value / 1024 ** index;
  return `${amount >= 10 || index === 0 ? amount.toFixed(0) : amount.toFixed(1)} ${units[index]}`;
}

function formatDate(value: string) {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
}

function formatChatTime(value: string) {
  const date = new Date(value);
  return Number.isNaN(date.getTime())
    ? value
    : date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

function upsertChatMessage(messages: ChatMessage[], next: ChatMessage) {
  const existing = messages.findIndex((message) => message.id === next.id);
  const updated = existing >= 0
    ? messages.map((message, index) => index === existing ? next : message)
    : [...messages, next];
  return updated.sort((left, right) => left.sent_at.localeCompare(right.sent_at));
}

function deliveryLabel(status: string, t: (key: string) => string) {
  if (status === "delivered") return t("Entregado");
  if (status === "partial") return t("Entrega parcial");
  if (status === "failed") return t("Falló");
  return t("Enviando");
}

function errorMessage(cause: unknown) {
  if (cause instanceof Error) return cause.message;
  if (typeof cause === "string") return cause;
  try {
    return JSON.stringify(cause);
  } catch {
    return "Error desconocido";
  }
}
