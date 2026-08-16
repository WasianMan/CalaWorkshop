import { faBoxArchive, faBroom, faDownload, faPlus, faRotate, faRotateRight, faSearch, faTrash } from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import {
  ActionIcon,
  Alert,
  Badge,
  Button,
  Card,
  Group,
  Image,
  Loader,
  Menu,
  Select,
  SegmentedControl,
  SimpleGrid,
  Stack,
  Switch,
  Table,
  Text,
  TextInput,
  Title,
} from '@mantine/core';
import { useEffect, useMemo, useRef, useState } from 'react';
import { httpErrorToHuman } from '@/api/axios.ts';
import { installCollection, previewCollection, type CollectionPreview } from '../api/collections.ts';
import cancelDownloads from '../api/cancelDownloads.ts';
import clearDownloads from '../api/clearDownloads.ts';
import deleteDownload from '../api/deleteDownload.ts';
import deleteInstalled from '../api/deleteInstalled.ts';
import archiveInstalled from '../api/archiveInstalled.ts';
import listArchives, { type WorkshopArchive } from '../api/listArchives.ts';
import removeAllInstalled from '../api/removeAllInstalled.ts';
import restoreArchive from '../api/restoreArchive.ts';
import retryFailedDownloads from '../api/retryFailedDownloads.ts';
import getConfig, { type GamePreset, type WorkshopConfig } from '../api/getConfig.ts';
import getJob from '../api/getJob.ts';
import importInstalled from '../api/importInstalled.ts';
import installJob from '../api/installJob.ts';
import listDownloads from '../api/listDownloads.ts';
import listInstalled, { type InstalledEntry } from '../api/listInstalled.ts';
import searchWorkshop, {
  type WorkshopFileType,
  type WorkshopSearchItem,
  type WorkshopSearchSort,
} from '../api/searchWorkshop.ts';
import startDownload from '../api/startDownload.ts';
import listAccounts from '../api/steam/listAccounts.ts';
import ServerContentContainer from '@/elements/containers/ServerContentContainer.tsx';
import { ServerCan } from '@/elements/Can.tsx';
import { useToast } from '@/providers/ToastProvider.tsx';
import { useServerStore } from '@/stores/server.ts';

type JobRow = {
  id: string;
  workshopId: number;
  title?: string | null;
  state: string;
  fileName?: string | null;
  error?: string | null;
};

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const PAGE_SIZE_OPTIONS = [5, 10, 15, 25];

/** Slow per-job polling down as the active batch grows, to spare the backend. */
const pollIntervalMs = (activeCount: number) =>
  activeCount <= 10 ? 2000 : activeCount <= 40 ? 4000 : activeCount <= 120 ? 6000 : 10000;

/**
 * Above this many in-flight jobs, don't auto-resume polling on page load — a
 * large stale backlog (e.g. from a crashed run) would otherwise hammer the
 * backend the instant the page opens. The user resumes or cancels explicitly.
 */
const RESUME_AUTOPOLL_LIMIT = 15;

/** Max simultaneous install (Wings pull/decompress) operations. A big batch of
 * already-`ready` jobs would otherwise fire dozens of volume ops at once. */
const MAX_INSTALL_CONCURRENCY = 3;

/** Above this many tracked jobs, suppress per-item install toasts (the lists
 * already reflect state) so a large batch can't spam dozens of notifications. */
const TOAST_BATCH_LIMIT = 10;

/** A download is "active" until it reaches a terminal state. */
const TERMINAL_STATES = new Set(['installed', 'failed']);
const isActiveState = (state: string) => !TERMINAL_STATES.has(state);

function parseWorkshopId(input: string): number | null {
  const trimmed = input.trim();
  const fromQuery = trimmed.match(/[?&]id=(\d+)/);
  if (fromQuery) return Number(fromQuery[1]);
  const digits = trimmed.match(/(\d{4,})/);
  if (digits) return Number(digits[1]);
  return null;
}

function formatBytes(value?: number | null): string {
  if (!value) return '';
  const units = ['B', 'KB', 'MB', 'GB'];
  let size = value;
  let idx = 0;
  while (size >= 1024 && idx < units.length - 1) {
    size /= 1024;
    idx += 1;
  }
  return `${size.toFixed(idx === 0 ? 0 : 1)} ${units[idx]}`;
}

function stars(value?: number | null): string {
  if (value == null) return 'No votes';
  return `${value.toFixed(1)} / 5`;
}

function defaultSearchTags(appId?: number): string[] {
  return appId === 4000 ? ['Addon'] : [];
}

function requiresAccountForPreset(preset: GamePreset, defaultAnonymous: boolean): boolean {
  return preset.auth === 'account' || ((preset.auth ?? 'default') === 'default' && !defaultAnonymous);
}

export default function WorkshopPage() {
  const server = useServerStore((s) => s.server);
  const { addToast } = useToast();

  const [config, setConfig] = useState<WorkshopConfig | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [presetIndex, setPresetIndex] = useState<number | null>(null);
  const [installPath, setInstallPath] = useState('');
  const [mode, setMode] = useState<'direct' | 'search' | 'collection'>('direct');
  const [workshopInput, setWorkshopInput] = useState('');
  const [collectionInput, setCollectionInput] = useState('');
  const [archive, setArchive] = useState(false);
  const [account, setAccount] = useState<string | null>(null);
  const [accounts, setAccounts] = useState<string[]>([]);
  const [submitting, setSubmitting] = useState(false);
  const [jobs, setJobs] = useState<JobRow[]>([]);
  const [installed, setInstalled] = useState<InstalledEntry[]>([]);
  const [installedLoading, setInstalledLoading] = useState(false);
  const [searchQuery, setSearchQuery] = useState('');
  const [searchSort, setSearchSort] = useState<WorkshopSearchSort>('popular');
  const [searchFileType, setSearchFileType] = useState<WorkshopFileType>('item');
  const [searchPerPage, setSearchPerPage] = useState(15);
  const [selectedTags, setSelectedTags] = useState<string[]>([]);
  const [showAllTags, setShowAllTags] = useState(false);
  const [searchLoading, setSearchLoading] = useState(false);
  const [searchError, setSearchError] = useState<string | null>(null);
  const [searchResults, setSearchResults] = useState<WorkshopSearchItem[]>([]);
  const [nextCursor, setNextCursor] = useState<string | null>(null);
  const [collectionPreview, setCollectionPreview] = useState<CollectionPreview | null>(null);
  const [collectionLoading, setCollectionLoading] = useState(false);
  const [history, setHistory] = useState<JobRow[]>([]);
  const [historyTotal, setHistoryTotal] = useState(0);
  const [historyPage, setHistoryPage] = useState(1);
  const [historyPerPage, setHistoryPerPage] = useState(25);
  const [retrying, setRetrying] = useState(false);
  const [clearing, setClearing] = useState(false);
  const [removingAll, setRemovingAll] = useState(false);
  const [archiving, setArchiving] = useState(false);
  const [restoring, setRestoring] = useState(false);
  const [cancelling, setCancelling] = useState(false);
  const [pendingResumeCount, setPendingResumeCount] = useState(0);
  const [archives, setArchives] = useState<WorkshopArchive[]>([]);
  const [selectedArchive, setSelectedArchive] = useState<string | null>(null);

  // Latest install path, read at install time so jobs resumed after a reload
  // (whose path wasn't known when polling started) still install correctly.
  const installPathRef = useRef('');
  // History page we last loaded, so background refreshes stay on the same page.
  const historyPageRef = useRef(1);
  // Job ids with a live poll loop, to avoid double-polling/double-installing.
  const polledIds = useRef<Set<string>>(new Set());
  // Live count of in-flight install operations, for pacing.
  const installSlots = useRef(0);
  // Warn at most once when downloads are ready but no install path is known.
  const warnedNoPathRef = useRef(false);

  const updateJob = (id: string, patch: Partial<JobRow>) =>
    setJobs((prev) => prev.map((j) => (j.id === id ? { ...j, ...patch } : j)));

  const toJobRow = (job: {
    id: string;
    workshopId: number;
    title?: string | null;
    state: string;
    fileName?: string | null;
    error?: string | null;
  }): JobRow => ({
    id: job.id,
    workshopId: job.workshopId,
    title: job.title,
    state: job.state,
    fileName: job.fileName,
    error: job.error,
  });

  const loadArchives = () => {
    listArchives(server.uuid)
      .then((list) => {
        setArchives(list);
        setSelectedArchive((prev) => prev ?? list[0]?.file ?? null);
      })
      .catch(() => setArchives([]));
  };

  const loadInstalled = () => {
    setInstalledLoading(true);
    listInstalled(server.uuid)
      .then(setInstalled)
      .catch(() => setInstalled([]))
      .finally(() => setInstalledLoading(false));
    loadArchives();
  };

  useEffect(() => {
    getConfig(server.uuid)
      .then((cfg) => {
        setConfig(cfg);
        const detectedIdx =
          cfg.detectedAppId != null && cfg.detectedAppIdConfidence !== 'low'
            ? cfg.presets.findIndex((p) => p.appId === cfg.detectedAppId)
            : -1;
        if (detectedIdx >= 0) {
          setPresetIndex(detectedIdx);
          setInstallPath(cfg.presets[detectedIdx].installPath);
          setSelectedTags(defaultSearchTags(cfg.presets[detectedIdx].appId));
          if (requiresAccountForPreset(cfg.presets[detectedIdx], cfg.defaultAnonymous)) setAccount(null);
        } else {
          setPresetIndex(null);
          setInstallPath('');
          setSelectedTags([]);
        }
        if (cfg.canLinkSteam) {
          listAccounts()
            .then((list) => setAccounts(list.map((a) => a.label)))
            .catch(() => setAccounts([]));
        }
      })
      .catch((err) => setLoadError(httpErrorToHuman(err)));

    // Downloads (active + history) are loaded by a dedicated effect below, which
    // also resumes polling for jobs still in flight after a page reload.
    loadInstalled();
    // biome-ignore lint/correctness/useExhaustiveDependencies: load once per server
  }, [server.uuid]);

  // Keep the install-path ref current for jobs whose poll started before the
  // path was known (e.g. resumed after a reload).
  useEffect(() => {
    installPathRef.current = installPath;
    if (installPath.trim()) warnedNoPathRef.current = false;
  }, [installPath]);

  const preset = useMemo(
    () => (presetIndex == null ? null : config?.presets[presetIndex] ?? null),
    [config, presetIndex],
  );
  const auth = preset?.auth ?? 'default';
  const accountRequired = auth === 'account' || (auth === 'default' && config?.defaultAnonymous === false);
  const canUseGame = !!preset && !!installPath.trim();
  const discoveredTags = useMemo(() => {
    const counts = new Map<string, number>();
    for (const item of searchResults) {
      for (const tag of item.tags ?? []) {
        const clean = tag.trim();
        if (!clean) continue;
        counts.set(clean, (counts.get(clean) ?? 0) + 1);
      }
    }
    return [...counts.entries()]
      .sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))
      .map(([tag]) => tag);
  }, [searchResults]);
  const visibleTags = showAllTags ? discoveredTags : discoveredTags.slice(0, 12);
  const installedIds = useMemo(
    () => new Set(installed.map((item) => item.workshopId).filter((id): id is number => typeof id === 'number')),
    [installed],
  );

  const pollJob = async (jobId: string) => {
    try {
      for (;;) {
        await sleep(pollIntervalMs(polledIds.current.size));
        let job;
        try {
          job = await getJob(server.uuid, jobId);
        } catch (err) {
          updateJob(jobId, { state: 'failed', error: httpErrorToHuman(err) });
          return;
        }
        updateJob(jobId, { state: job.state, fileName: job.fileName, error: job.error });

        if (job.state === 'failed') {
          addToast(job.error ?? 'Download failed', 'error');
          return;
        }
        if (job.state === 'ready') {
          // Prefer the path persisted on the job (survives reloads); fall back to
          // the current UI field for older jobs created before paths were stored.
          const path = (job.installPath ?? installPathRef.current ?? '').trim();
          if (!path) {
            // Don't fail or spam — leave the job `ready` and stop polling. Warn once.
            if (!warnedNoPathRef.current) {
              warnedNoPathRef.current = true;
              addToast(
                'Downloads are ready but no install path is set — select the game / set a path, then Resume.',
                'error',
              );
            }
            return;
          }
          // Pace installs so a big resumed backlog doesn't fire dozens of Wings
          // operations at once.
          while (installSlots.current >= MAX_INSTALL_CONCURRENCY) await sleep(400);
          installSlots.current += 1;
          updateJob(jobId, { state: 'installing' });
          const quiet = polledIds.current.size > TOAST_BATCH_LIMIT;
          try {
            const result = await installJob(server.uuid, jobId, path);
            updateJob(jobId, { state: 'installed', fileName: result.fileName });
            if (!quiet) addToast(`Installed ${result.files?.join(', ') || result.fileName}`, 'success');
            loadInstalled();
          } catch (err) {
            updateJob(jobId, { state: 'failed', error: httpErrorToHuman(err) });
            if (!quiet) addToast(httpErrorToHuman(err), 'error');
          } finally {
            installSlots.current -= 1;
          }
          return;
        }
      }
    } finally {
      // Release the poll slot and resync lists so the finished job moves from
      // the active card into the (terminal) history page.
      polledIds.current.delete(jobId);
      void loadDownloads(historyPageRef.current);
    }
  };

  /** Start a poll loop for a job unless one is already running for it. */
  const startPoll = (jobId: string) => {
    if (polledIds.current.has(jobId)) return;
    polledIds.current.add(jobId);
    void pollJob(jobId);
  };

  /**
   * Load active jobs + one page of history from the server, seed the lists, and
   * resume polling any active job we aren't already tracking (e.g. after reload).
   */
  const loadDownloads = async (page = historyPageRef.current, resume = false) => {
    let data;
    try {
      data = await listDownloads(server.uuid, page);
    } catch {
      return;
    }
    setJobs(data.active.map(toJobRow));
    setHistory(data.history.map(toJobRow));
    setHistoryTotal(data.historyTotal);
    setHistoryPage(data.page);
    setHistoryPerPage(data.perPage);
    historyPageRef.current = data.page;
    // Only the resume path (page load / retry) may start NEW poll loops. Background
    // refreshes (a job finishing, paging history) must not, or they'd re-spawn
    // polls during a legit batch. A large backlog is gated behind a manual resume.
    if (resume) {
      const active = data.active.filter((job) => isActiveState(job.state));
      if (active.length > RESUME_AUTOPOLL_LIMIT) {
        setPendingResumeCount(active.length);
      } else {
        setPendingResumeCount(0);
        for (const job of active) startPoll(job.id);
      }
    }
  };

  const resumePendingPolls = () => {
    warnedNoPathRef.current = false; // allow the no-path warning to show once more
    for (const job of jobs) {
      if (isActiveState(job.state)) startPoll(job.id);
    }
    setPendingResumeCount(0);
  };

  const handleCancelAll = async () => {
    if (
      !window.confirm(
        'Cancel all active and queued downloads? They will be marked failed. Files already installed are not touched.',
      )
    ) {
      return;
    }
    setCancelling(true);
    try {
      const n = await cancelDownloads(server.uuid);
      addToast(`Cancelled ${n} active download${n === 1 ? '' : 's'}`, 'success');
      setPendingResumeCount(0);
      await loadDownloads(1, false);
    } catch (err) {
      addToast(httpErrorToHuman(err), 'error');
    } finally {
      setCancelling(false);
    }
  };

  // Initial load (and resume of in-flight jobs) once per server.
  useEffect(() => {
    void loadDownloads(1, true);
    // biome-ignore lint/correctness/useExhaustiveDependencies: load once per server
  }, [server.uuid]);

  const handleRetryFailed = async () => {
    setRetrying(true);
    try {
      const res = await retryFailedDownloads(server.uuid, config?.canLinkSteam ? account : null);
      if (res.retried === 0 && res.stillFailed === 0) {
        addToast('No failed downloads to retry', 'info');
      } else {
        addToast(
          `Retrying ${res.retried} item${res.retried === 1 ? '' : 's'}` +
            (res.stillFailed ? `, ${res.stillFailed} could not be re-queued` : ''),
          res.retried > 0 ? 'success' : 'warning',
        );
      }
      await loadDownloads(1, true);
    } catch (err) {
      addToast(httpErrorToHuman(err), 'error');
    } finally {
      setRetrying(false);
    }
  };

  const handleClearHistory = async (stateFilter?: 'installed' | 'failed') => {
    const what =
      stateFilter === 'failed'
        ? 'all failed entries'
        : stateFilter === 'installed'
          ? 'all completed entries'
          : 'the entire download history';
    if (
      !window.confirm(
        `Clear ${what}? This only forgets history — installed files are not touched.`,
      )
    ) {
      return;
    }
    setClearing(true);
    try {
      const n = await clearDownloads(server.uuid, stateFilter);
      addToast(`Cleared ${n} ${n === 1 ? 'entry' : 'entries'}`, 'success');
      await loadDownloads(1, false);
    } catch (err) {
      addToast(httpErrorToHuman(err), 'error');
    } finally {
      setClearing(false);
    }
  };

  const handleArchiveAll = async () => {
    const suggested = `calaworkshop-archive-${new Date().toISOString().slice(0, 19).replace(/[:T]/g, '-')}`;
    const name = window.prompt('Name this archive (.tar.gz is added automatically):', suggested);
    if (name === null) return; // cancelled
    setArchiving(true);
    try {
      const res = await archiveInstalled(server.uuid, name);
      addToast(
        res.archived > 0
          ? `Archived ${res.archived} item${res.archived === 1 ? '' : 's'} to ${res.file}`
          : 'Nothing to archive',
        res.archived > 0 ? 'success' : 'info',
      );
      loadArchives();
    } catch (err) {
      addToast(httpErrorToHuman(err), 'error');
    } finally {
      setArchiving(false);
    }
  };

  const handleRestore = async () => {
    if (!selectedArchive) return;
    if (
      !window.confirm(
        `Restore "${selectedArchive}"? This unpacks the archived files back into the server volume and re-tracks them.`,
      )
    ) {
      return;
    }
    setRestoring(true);
    try {
      const res = await restoreArchive(server.uuid, selectedArchive);
      addToast(`Restored ${res.restored} item${res.restored === 1 ? '' : 's'} from ${res.file}`, 'success');
      loadInstalled();
    } catch (err) {
      addToast(httpErrorToHuman(err), 'error');
    } finally {
      setRestoring(false);
    }
  };

  const handleRemoveAll = async () => {
    if (
      !window.confirm(
        'Remove ALL tracked Workshop content from this server? This deletes the installed files from the server volume and cannot be undone.',
      )
    ) {
      return;
    }
    setRemovingAll(true);
    try {
      const removed = await removeAllInstalled(server.uuid);
      addToast(`Removed ${removed} installed item${removed === 1 ? '' : 's'}`, 'success');
      loadInstalled();
    } catch (err) {
      addToast(httpErrorToHuman(err), 'error');
    } finally {
      setRemovingAll(false);
    }
  };

  const startAndPoll = async (workshopId: number, title?: string | null) => {
    if (!preset) {
      addToast('Select a game first', 'error');
      return;
    }
    const path = installPath.trim();
    if (!path) {
      addToast('Install path is required', 'error');
      return;
    }
    if (accountRequired && !account) {
      addToast('Select a linked Steam account for this game', 'error');
      return;
    }
    const { jobId, state } = await startDownload(server.uuid, {
      appId: preset.appId,
      workshopId,
      account: config?.canLinkSteam ? account : null,
      archive,
      installPath: path,
    });
    setJobs((prev) => [{ id: jobId, workshopId, state, title }, ...prev]);
    startPoll(jobId);
  };

  const handleInstall = async () => {
    const workshopId = parseWorkshopId(workshopInput);
    if (!workshopId) {
      addToast('Could not read a Workshop ID from that input', 'error');
      return;
    }
    setSubmitting(true);
    try {
      await startAndPoll(workshopId);
      setWorkshopInput('');
    } catch (err) {
      addToast(httpErrorToHuman(err), 'error');
    } finally {
      setSubmitting(false);
    }
  };

  const runSearch = async (cursor?: string | null) => {
    if (!preset || !config?.steamSearchAvailable) return;
    setSearchLoading(true);
    setSearchError(null);
    try {
      const result = await searchWorkshop(server.uuid, {
        appId: preset.appId,
        query: searchQuery,
        sort: searchQuery.trim() ? searchSort : searchSort === 'relevance' ? 'popular' : searchSort,
        cursor,
        fileType: searchFileType,
        tags: selectedTags,
        perPage: searchPerPage,
      });
      setSearchResults((prev) => (cursor ? [...prev, ...result.items] : result.items));
      setNextCursor(result.nextCursor ?? null);
    } catch (err) {
      setSearchError(httpErrorToHuman(err));
      if (!cursor) setSearchResults([]);
    } finally {
      setSearchLoading(false);
    }
  };

  useEffect(() => {
    if (mode !== 'search' || !preset || !config?.steamSearchAvailable) return;
    const timer = window.setTimeout(() => {
      void runSearch(null);
    }, 400);
    return () => window.clearTimeout(timer);
    // biome-ignore lint/correctness/useExhaustiveDependencies: debounced search inputs only
  }, [mode, preset?.appId, searchQuery, searchSort, searchFileType, searchPerPage, selectedTags.join('|'), config?.steamSearchAvailable]);

  const toggleTag = (tag: string) => {
    if (preset?.appId === 4000 && searchFileType === 'item' && tag.toLowerCase() === 'addon') {
      return;
    }
    setSelectedTags((prev) => (prev.includes(tag) ? prev.filter((item) => item !== tag) : [...prev, tag]));
  };

  const clearSearchFilters = () => {
    setSearchQuery('');
    setSearchSort('popular');
    setSearchFileType('item');
    setSearchPerPage(15);
    setSelectedTags(defaultSearchTags(preset?.appId));
    setShowAllTags(false);
  };

  const previewCollectionId = async (collectionId: number) => {
    if (!preset) {
      addToast('Select a game first', 'error');
      return;
    }
    setCollectionLoading(true);
    try {
      setCollectionInput(String(collectionId));
      setCollectionPreview(await previewCollection(server.uuid, { appId: preset.appId, collectionId }));
      setMode('collection');
    } catch (err) {
      addToast(httpErrorToHuman(err), 'error');
      setCollectionPreview(null);
    } finally {
      setCollectionLoading(false);
    }
  };

  const handleCollectionPreview = async () => {
    if (!preset) {
      addToast('Select a game first', 'error');
      return;
    }
    const collectionId = parseWorkshopId(collectionInput);
    if (!collectionId) {
      addToast('Could not read a collection ID from that input', 'error');
      return;
    }
    await previewCollectionId(collectionId);
  };

  const handleCollectionInstall = async () => {
    if (!preset || !collectionPreview) return;
    const collectionId = parseWorkshopId(collectionInput);
    if (!collectionId) return;
    const path = installPath.trim();
    if (!path) {
      addToast('Install path is required', 'error');
      return;
    }
    if (accountRequired && !account) {
      addToast('Select a linked Steam account for this game', 'error');
      return;
    }
    setSubmitting(true);
    try {
      const result = await installCollection(server.uuid, {
        appId: preset.appId,
        collectionId,
        account: config?.canLinkSteam ? account : null,
        installPath: path,
      });
      const rows = result.jobs.map((job, index) => ({
        id: (job as any).jobId ?? (job as any).job_id,
        workshopId: collectionPreview.children[index]?.publishedFileId ?? 0,
        title: collectionPreview.children[index]?.title,
        state: job.state,
      }));
      setJobs((prev) => [...rows, ...prev]);
      for (const row of rows) {
        if (row.id) startPoll(row.id);
      }
      const alreadyInstalled = (result.skipped ?? []).filter((s) =>
        s.reason?.toLowerCase().includes('already installed'),
      ).length;
      const skippedNote = alreadyInstalled > 0 ? `, skipped ${alreadyInstalled} already installed` : '';
      addToast(
        rows.length > 0
          ? `Queued ${rows.length} collection item${rows.length === 1 ? '' : 's'}${skippedNote}`
          : `Nothing to download${skippedNote || ' — collection already installed'}`,
        rows.length > 0 ? 'success' : 'info',
      );
    } catch (err) {
      addToast(httpErrorToHuman(err), 'error');
    } finally {
      setSubmitting(false);
    }
  };

  const handleDelete = async (entry: InstalledEntry) => {
    if (!entry.id) return;
    try {
      await deleteInstalled(server.uuid, entry.id);
      addToast(`Removed ${entry.title}`, 'success');
      loadInstalled();
    } catch (err) {
      addToast(httpErrorToHuman(err), 'error');
    }
  };

  const handleDeleteJob = async (job: JobRow) => {
    try {
      await deleteDownload(server.uuid, job.id);
      polledIds.current.delete(job.id);
      setJobs((prev) => prev.filter((j) => j.id !== job.id));
      setHistory((prev) => prev.filter((j) => j.id !== job.id));
      setHistoryTotal((prev) => Math.max(0, prev - 1));
    } catch (err) {
      addToast(httpErrorToHuman(err), 'error');
    }
  };

  const historyTotalPages = Math.max(1, Math.ceil(historyTotal / historyPerPage));
  const activeJobs = jobs.filter((job) => isActiveState(job.state));

  const handleTrack = async (entry: InstalledEntry) => {
    try {
      await importInstalled(server.uuid, entry);
      addToast(`Tracking ${entry.title}`, 'success');
      loadInstalled();
    } catch (err) {
      addToast(httpErrorToHuman(err), 'error');
    }
  };

  const stateColor = (state: string) =>
    state === 'installed' ? 'green' : state === 'failed' ? 'red' : state === 'ready' || state === 'installing' ? 'blue' : 'gray';

  const renderWorkshopCard = (item: WorkshopSearchItem, action: 'install' | 'collection' | 'none' = 'install') => {
    const installable = !(preset?.appId === 4000 && !item.tags.some((tag) => tag.toLowerCase() === 'addon'));
    const installedAlready = installedIds.has(item.publishedFileId);
    return (
      <Card withBorder radius='md' padding='sm' key={item.publishedFileId}>
        <Group align='flex-start' wrap='nowrap'>
          {item.previewUrl ? <Image src={item.previewUrl} w={96} h={72} fit='cover' radius='sm' /> : null}
          <Stack gap={4} style={{ flex: 1, minWidth: 0 }}>
            <Text fw={600} lineClamp={1}>{item.title}</Text>
            <Text size='xs' c='dimmed'>
              {stars(item.stars)}{item.voteCount ? ` · ${item.voteCount} votes` : ''}{item.subscriptions ? ` · ${item.subscriptions.toLocaleString()} subs` : ''}{item.fileSize ? ` · ${formatBytes(item.fileSize)}` : ''}
            </Text>
            {item.tags.length > 0 ? (
              <Group gap={4}>
                {item.tags.slice(0, 5).map((tag) => (
                  <Badge key={tag} size='xs' variant='light'>{tag}</Badge>
                ))}
              </Group>
            ) : null}
            {item.shortDescription ? <Text size='xs' c='dimmed' lineClamp={2}>{item.shortDescription}</Text> : null}
          </Stack>
          {action === 'install' ? (
            <Button
              size='xs'
              leftSection={<FontAwesomeIcon icon={faDownload} />}
              onClick={() => void startAndPoll(item.publishedFileId, item.title)}
              disabled={!installable || installedAlready}
            >
              {installedAlready ? 'Installed' : installable ? 'Install' : 'Not addon'}
            </Button>
          ) : action === 'collection' ? (
            <Button size='xs' leftSection={<FontAwesomeIcon icon={faSearch} />} onClick={() => void previewCollectionId(item.publishedFileId)}>
              Preview
            </Button>
          ) : null}
        </Group>
      </Card>
    );
  };

  return (
    <ServerContentContainer title='Workshop'>
      <Stack gap='md'>
        {loadError ? <Alert color='red' title='Failed to load'>{loadError}</Alert> : null}
        {config && !config.helperConfigured ? (
          <Alert color='yellow' title='Helper not configured'>
            An administrator needs to set the workshop helper URL and token in the extension settings before downloads will work.
          </Alert>
        ) : null}

        <ServerCan action='workshop.install'>
          <Card withBorder radius='md' padding='lg'>
            <Stack gap='sm'>
              <Title order={4}>Install Workshop content</Title>
              <Group grow align='end'>
                <Select
                  label='Game'
                  placeholder='Select a game'
                  data={(config?.presets ?? []).map((p, i) => ({ value: String(i), label: p.name }))}
                  value={presetIndex == null ? null : String(presetIndex)}
                  onChange={(v) => {
                    if (v == null) {
                      setPresetIndex(null);
                      setInstallPath('');
                      return;
                    }
                    const idx = Number(v);
                    setPresetIndex(idx);
                    if (config?.presets[idx]) {
                      setInstallPath(config.presets[idx].installPath);
                      setSelectedTags(defaultSearchTags(config.presets[idx].appId));
                      if (requiresAccountForPreset(config.presets[idx], config.defaultAnonymous)) setAccount(null);
                    }
                  }}
                />
                <TextInput label='Install path' value={installPath} onChange={(e) => setInstallPath(e.currentTarget.value)} />
              </Group>
              {config?.detectedAppId != null && config.detectedAppIdConfidence === 'low' ? (
                <Text size='xs' c='dimmed'>Possible game match: {config.detectedAppId}, but confidence is low. Select the game manually.</Text>
              ) : config?.detectedAppId != null && preset?.appId === config.detectedAppId ? (
                <Text size='xs' c='dimmed'>
                  Auto-selected from this server&apos;s game ({config.detectedAppIdConfidence} confidence). Change it above if needed.
                </Text>
              ) : null}
              {config?.canLinkSteam ? (
                <Select
                  label='Steam account'
                  data={[
                    ...(accountRequired ? [] : [{ value: '', label: 'Anonymous' }]),
                    ...accounts.map((a) => ({ value: a, label: a })),
                  ]}
                  value={accountRequired ? account : account ?? ''}
                  onChange={(v) => setAccount(v ? v : null)}
                  w={240}
                />
              ) : null}

              <SegmentedControl
                value={mode}
                onChange={(v) => setMode(v as typeof mode)}
                data={[
                  { value: 'direct', label: 'Direct' },
                  { value: 'search', label: 'Search' },
                  { value: 'collection', label: 'Collection' },
                ]}
              />

              {mode === 'direct' ? (
                <Stack gap='sm'>
                  <TextInput
                    label='Workshop URL or ID'
                    placeholder='https://steamcommunity.com/sharedfiles/filedetails/?id=123456789'
                    value={workshopInput}
                    onChange={(e) => setWorkshopInput(e.currentTarget.value)}
                  />
                  <Group align='end'>
                    <Switch label='Archive whole item' checked={archive} onChange={(e) => setArchive(e.currentTarget.checked)} />
                    <Button
                      leftSection={<FontAwesomeIcon icon={faDownload} />}
                      loading={submitting}
                      onClick={handleInstall}
                      disabled={!config?.helperConfigured || !canUseGame}
                    >
                      Download &amp; install
                    </Button>
                  </Group>
                </Stack>
              ) : null}

              {mode === 'search' ? (
                <Stack gap='sm'>
                  {!config?.steamSearchAvailable ? <Alert color='yellow'>Steam Web API key is required for search.</Alert> : null}
                  <Group grow align='end'>
                    <TextInput
                      label='Search'
                      placeholder='Leave blank to explore popular items'
                      value={searchQuery}
                      onChange={(e) => setSearchQuery(e.currentTarget.value)}
                      leftSection={<FontAwesomeIcon icon={faSearch} />}
                    />
                    <Select
                      label='Sort'
                      value={searchSort}
                      onChange={(v) => setSearchSort((v ?? 'popular') as WorkshopSearchSort)}
                      data={[
                        { value: 'relevance', label: 'Relevance' },
                        { value: 'popular', label: 'Popular' },
                        { value: 'trending', label: 'Trending' },
                        { value: 'newest', label: 'Newest' },
                        { value: 'updated', label: 'Recently updated' },
                        { value: 'subscribed', label: 'Most subscribed' },
                      ]}
                    />
                    <Select
                      label='Per page'
                      value={String(searchPerPage)}
                      onChange={(v) => setSearchPerPage(Number(v ?? 15))}
                      data={PAGE_SIZE_OPTIONS.map((size) => ({ value: String(size), label: String(size) }))}
                    />
                  </Group>
                  <Group justify='space-between' align='end'>
                    <SegmentedControl
                      value={searchFileType}
                      onChange={(v) => {
                        const nextType = v as WorkshopFileType;
                        setSearchFileType(nextType);
                        setSelectedTags(nextType === 'item' ? defaultSearchTags(preset?.appId) : []);
                        setShowAllTags(false);
                      }}
                      data={[
                        { value: 'item', label: 'Items' },
                        { value: 'collection', label: 'Collections' },
                      ]}
                    />
                    <Button variant='subtle' size='xs' onClick={clearSearchFilters}>
                      Reset filters
                    </Button>
                  </Group>
                  {selectedTags.length > 0 || discoveredTags.length > 0 ? (
                    <Stack gap={6}>
                      {selectedTags.length > 0 ? (
                        <Group gap={6}>
                          <Text size='xs' c='dimmed'>Selected</Text>
                          {selectedTags.map((tag) => (
                            <Badge key={tag} variant='filled' onClick={() => toggleTag(tag)} style={{ cursor: 'pointer' }}>
                              {tag}
                            </Badge>
                          ))}
                        </Group>
                      ) : null}
                      {discoveredTags.length > 0 ? (
                        <Group gap={6}>
                          <Text size='xs' c='dimmed'>Tags</Text>
                          {visibleTags.map((tag) => (
                            <Badge
                              key={tag}
                              variant={selectedTags.includes(tag) ? 'filled' : 'light'}
                              onClick={() => toggleTag(tag)}
                              style={{ cursor: 'pointer' }}
                            >
                              {tag}
                            </Badge>
                          ))}
                          {discoveredTags.length > 12 ? (
                            <Button variant='subtle' size='xs' onClick={() => setShowAllTags((v) => !v)}>
                              {showAllTags ? 'Show fewer' : `Show ${discoveredTags.length - 12} more`}
                            </Button>
                          ) : null}
                        </Group>
                      ) : null}
                    </Stack>
                  ) : null}
                  {searchError ? <Alert color='red'>{searchError}</Alert> : null}
                  {searchLoading && searchResults.length === 0 ? <Loader size='sm' /> : null}
                  <Stack gap='xs'>
                    {searchResults.map((item) => renderWorkshopCard(item, searchFileType === 'collection' ? 'collection' : 'install'))}
                  </Stack>
                  {nextCursor ? (
                    <Button variant='subtle' loading={searchLoading} onClick={() => void runSearch(nextCursor)} disabled={!canUseGame}>
                      Load more
                    </Button>
                  ) : null}
                </Stack>
              ) : null}

              {mode === 'collection' ? (
                <Stack gap='sm'>
                  <Group grow align='end'>
                    <TextInput
                      label='Collection URL or ID'
                      placeholder='https://steamcommunity.com/sharedfiles/filedetails/?id=123456789'
                      value={collectionInput}
                      onChange={(e) => setCollectionInput(e.currentTarget.value)}
                    />
                    <Button loading={collectionLoading} onClick={handleCollectionPreview} disabled={!canUseGame}>
                      Preview collection
                    </Button>
                  </Group>
                  {collectionPreview ? (
                    <Stack gap='sm'>
                      <Text size='sm'>
                        {collectionPreview.collection?.title ?? 'Collection'} · {collectionPreview.children.length} installable items
                      </Text>
                      <SimpleGrid cols={{ base: 1, sm: 2, lg: 3 }}>
                        {collectionPreview.children.slice(0, 12).map((item) => renderWorkshopCard(item, 'none'))}
                      </SimpleGrid>
                      {collectionPreview.skipped.length > 0 ? (
                        <Alert color='yellow'>{collectionPreview.skipped.length} collection entries were skipped.</Alert>
                      ) : null}
                      <Button
                        leftSection={<FontAwesomeIcon icon={faDownload} />}
                        loading={submitting}
                        onClick={handleCollectionInstall}
                        disabled={!canUseGame || collectionPreview.children.length === 0}
                      >
                        Install collection
                      </Button>
                    </Stack>
                  ) : null}
                </Stack>
              ) : null}
            </Stack>
          </Card>
        </ServerCan>

        {pendingResumeCount > 0 ? (
          <Alert color='yellow' title={`${pendingResumeCount} downloads pending from a previous session`}>
            <Text size='sm' mb='xs'>
              Polling was paused so a large backlog doesn't overload the panel on page load. Resume to
              keep installing them, or cancel them all (already-installed files are untouched).
            </Text>
            <Group gap='xs'>
              <ServerCan action='workshop.install'>
                <Button size='xs' onClick={resumePendingPolls}>Resume</Button>
              </ServerCan>
              <ServerCan action='workshop.install'>
                <Button size='xs' color='red' variant='light' loading={cancelling} onClick={handleCancelAll}>
                  Cancel all
                </Button>
              </ServerCan>
            </Group>
          </Alert>
        ) : null}

        {activeJobs.length > 0 ? (
          <Card withBorder radius='md' padding='lg'>
            <Group justify='space-between' mb='sm'>
              <Title order={4}>Downloading ({activeJobs.length})</Title>
              <Group gap='sm'>
                <Text size='xs' c='dimmed'>Large collections are paced — items wait in “queued” until a download slot is free.</Text>
                <ServerCan action='workshop.install'>
                  <Button size='xs' color='red' variant='subtle' loading={cancelling} onClick={handleCancelAll}>
                    Cancel all
                  </Button>
                </ServerCan>
              </Group>
            </Group>
            <Table>
              <Table.Thead>
                <Table.Tr>
                  <Table.Th>Workshop</Table.Th>
                  <Table.Th>File</Table.Th>
                  <Table.Th>Status</Table.Th>
                  <Table.Th />
                </Table.Tr>
              </Table.Thead>
              <Table.Tbody>
                {activeJobs.map((job) => (
                  <Table.Tr key={job.id}>
                    <Table.Td>{job.title ?? (job.workshopId || job.id)}</Table.Td>
                    <Table.Td>{job.fileName ?? '-'}</Table.Td>
                    <Table.Td>
                      <Badge color={stateColor(job.state)}>{job.state}</Badge>
                      {job.error ? <Text size='xs' c='red'>{job.error}</Text> : null}
                    </Table.Td>
                    <Table.Td align='right'>
                      <ServerCan action='workshop.install'>
                        <ActionIcon color='red' variant='subtle' aria-label='Remove download' title='Remove download' onClick={() => handleDeleteJob(job)}>
                          <FontAwesomeIcon icon={faTrash} />
                        </ActionIcon>
                      </ServerCan>
                    </Table.Td>
                  </Table.Tr>
                ))}
              </Table.Tbody>
            </Table>
          </Card>
        ) : null}

        {history.length > 0 || historyTotal > 0 ? (
          <Card withBorder radius='md' padding='lg'>
            <Group justify='space-between' mb='sm'>
              <Title order={4}>Download history</Title>
              <ServerCan action='workshop.install'>
                <Group gap='xs'>
                  <Button
                    size='xs'
                    variant='light'
                    color='orange'
                    leftSection={<FontAwesomeIcon icon={faRotateRight} />}
                    loading={retrying}
                    onClick={handleRetryFailed}
                  >
                    Retry failed
                  </Button>
                  <Menu shadow='md' position='bottom-end'>
                    <Menu.Target>
                      <Button
                        size='xs'
                        variant='light'
                        color='red'
                        leftSection={<FontAwesomeIcon icon={faBroom} />}
                        loading={clearing}
                      >
                        Clear
                      </Button>
                    </Menu.Target>
                    <Menu.Dropdown>
                      <Menu.Item onClick={() => void handleClearHistory('failed')}>
                        Clear failed
                      </Menu.Item>
                      <Menu.Item onClick={() => void handleClearHistory('installed')}>
                        Clear completed
                      </Menu.Item>
                      <Menu.Item color='red' onClick={() => void handleClearHistory()}>
                        Clear all history
                      </Menu.Item>
                    </Menu.Dropdown>
                  </Menu>
                </Group>
              </ServerCan>
            </Group>
            <Table>
              <Table.Thead>
                <Table.Tr>
                  <Table.Th>Workshop</Table.Th>
                  <Table.Th>File</Table.Th>
                  <Table.Th>Status</Table.Th>
                  <Table.Th />
                </Table.Tr>
              </Table.Thead>
              <Table.Tbody>
                {history.map((job) => (
                  <Table.Tr key={job.id}>
                    <Table.Td>{job.title ?? (job.workshopId || job.id)}</Table.Td>
                    <Table.Td>{job.fileName ?? '-'}</Table.Td>
                    <Table.Td>
                      <Badge color={stateColor(job.state)}>{job.state}</Badge>
                      {job.error ? <Text size='xs' c='red'>{job.error}</Text> : null}
                    </Table.Td>
                    <Table.Td align='right'>
                      <ServerCan action='workshop.install'>
                        <ActionIcon color='red' variant='subtle' aria-label='Remove from history' title='Remove from history' onClick={() => handleDeleteJob(job)}>
                          <FontAwesomeIcon icon={faTrash} />
                        </ActionIcon>
                      </ServerCan>
                    </Table.Td>
                  </Table.Tr>
                ))}
              </Table.Tbody>
            </Table>
            {historyTotalPages > 1 ? (
              <Group justify='center' gap='sm' mt='sm'>
                <Button
                  size='xs'
                  variant='default'
                  disabled={historyPage <= 1}
                  onClick={() => void loadDownloads(historyPage - 1)}
                >
                  Previous
                </Button>
                <Text size='sm' c='dimmed'>Page {historyPage} of {historyTotalPages}</Text>
                <Button
                  size='xs'
                  variant='default'
                  disabled={historyPage >= historyTotalPages}
                  onClick={() => void loadDownloads(historyPage + 1)}
                >
                  Next
                </Button>
              </Group>
            ) : null}
          </Card>
        ) : null}

        <Card withBorder radius='md' padding='lg'>
          <Group justify='space-between' mb='sm'>
            <Title order={4}>Installed content</Title>
            <Group gap='xs'>
              {installed.some((entry) => entry.source !== 'unmanaged') ? (
                <>
                  <ServerCan action='workshop.install'>
                    <Button
                      variant='light'
                      leftSection={<FontAwesomeIcon icon={faBoxArchive} />}
                      loading={archiving}
                      onClick={handleArchiveAll}
                    >
                      Archive all
                    </Button>
                  </ServerCan>
                  <ServerCan action='workshop.remove'>
                    <Button
                      variant='light'
                      color='red'
                      leftSection={<FontAwesomeIcon icon={faTrash} />}
                      loading={removingAll}
                      onClick={handleRemoveAll}
                    >
                      Remove all
                    </Button>
                  </ServerCan>
                </>
              ) : null}
              <Button variant='subtle' leftSection={<FontAwesomeIcon icon={faRotate} />} onClick={loadInstalled}>Refresh</Button>
            </Group>
          </Group>
          {archives.length > 0 ? (
            <ServerCan action='workshop.install'>
              <Group gap='xs' mb='sm' align='flex-end'>
                <Select
                  label='Restore from archive'
                  description='Unpacks a backup and re-tracks its items'
                  data={archives.map((a) => ({
                    value: a.file,
                    label: `${a.file} (${a.itemCount} item${a.itemCount === 1 ? '' : 's'})`,
                  }))}
                  value={selectedArchive}
                  onChange={setSelectedArchive}
                  searchable
                  style={{ flex: 1, maxWidth: 480 }}
                />
                <Button
                  variant='light'
                  color='blue'
                  leftSection={<FontAwesomeIcon icon={faRotateRight} />}
                  loading={restoring}
                  disabled={!selectedArchive}
                  onClick={handleRestore}
                >
                  Restore
                </Button>
              </Group>
            </ServerCan>
          ) : null}
          {installedLoading ? (
            <Loader size='sm' />
          ) : installed.length === 0 ? (
            <Text c='dimmed' size='sm'>No Workshop content found for this server.</Text>
          ) : (
            <Table>
              <Table.Thead>
                <Table.Tr>
                  <Table.Th>Item</Table.Th>
                  <Table.Th>Path</Table.Th>
                  <Table.Th>Source</Table.Th>
                  <Table.Th />
                </Table.Tr>
              </Table.Thead>
              <Table.Tbody>
                {installed.map((entry) => (
                  <Table.Tr key={`${entry.installPath}:${entry.files.join('|')}:${entry.id ?? 'unmanaged'}`}>
                    <Table.Td>
                      <Text fw={500}>{entry.title}</Text>
                      <Text size='xs' c='dimmed'>{entry.files.join(', ')}</Text>
                    </Table.Td>
                    <Table.Td>{entry.installPath}</Table.Td>
                    <Table.Td><Badge color={entry.source === 'unmanaged' ? 'yellow' : 'green'}>{entry.source}</Badge></Table.Td>
                    <Table.Td align='right'>
                      {entry.source === 'unmanaged' ? (
                        <ServerCan action='workshop.install'>
                          <Button size='xs' variant='subtle' leftSection={<FontAwesomeIcon icon={faPlus} />} onClick={() => handleTrack(entry)}>Track</Button>
                        </ServerCan>
                      ) : (
                        <ServerCan action='workshop.remove'>
                          <ActionIcon color='red' variant='subtle' onClick={() => handleDelete(entry)}>
                            <FontAwesomeIcon icon={faTrash} />
                          </ActionIcon>
                        </ServerCan>
                      )}
                    </Table.Td>
                  </Table.Tr>
                ))}
              </Table.Tbody>
            </Table>
          )}
        </Card>
      </Stack>
    </ServerContentContainer>
  );
}
