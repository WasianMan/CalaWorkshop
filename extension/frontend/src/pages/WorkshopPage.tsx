import { faDownload, faPlus, faRotate, faSearch, faTrash } from '@fortawesome/free-solid-svg-icons';
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
import { useEffect, useMemo, useState } from 'react';
import { httpErrorToHuman } from '@/api/axios.ts';
import { installCollection, previewCollection, type CollectionPreview } from '../api/collections.ts';
import deleteDownload from '../api/deleteDownload.ts';
import deleteInstalled from '../api/deleteInstalled.ts';
import getConfig, { type WorkshopConfig } from '../api/getConfig.ts';
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
  const [selectedTags, setSelectedTags] = useState<string[]>([]);
  const [showAllTags, setShowAllTags] = useState(false);
  const [searchLoading, setSearchLoading] = useState(false);
  const [searchError, setSearchError] = useState<string | null>(null);
  const [searchResults, setSearchResults] = useState<WorkshopSearchItem[]>([]);
  const [nextCursor, setNextCursor] = useState<string | null>(null);
  const [collectionPreview, setCollectionPreview] = useState<CollectionPreview | null>(null);
  const [collectionLoading, setCollectionLoading] = useState(false);

  const updateJob = (id: string, patch: Partial<JobRow>) =>
    setJobs((prev) => prev.map((j) => (j.id === id ? { ...j, ...patch } : j)));

  const loadInstalled = () => {
    setInstalledLoading(true);
    listInstalled(server.uuid)
      .then(setInstalled)
      .catch(() => setInstalled([]))
      .finally(() => setInstalledLoading(false));
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
        } else {
          setPresetIndex(null);
          setInstallPath('');
        }
        if (cfg.canLinkSteam) {
          listAccounts()
            .then((list) => setAccounts(list.map((a) => a.label)))
            .catch(() => setAccounts([]));
        }
      })
      .catch((err) => setLoadError(httpErrorToHuman(err)));

    listDownloads(server.uuid)
      .then((rows) =>
        setJobs(
          rows.map((job) => ({
            id: job.id,
            workshopId: job.workshopId,
            state: job.state,
            fileName: job.fileName,
            error: job.error,
            title: job.title,
          })),
        ),
      )
      .catch(() => setJobs([]));
    loadInstalled();
    // biome-ignore lint/correctness/useExhaustiveDependencies: load once per server
  }, [server.uuid]);

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

  const pollJob = async (jobId: string, path: string) => {
    for (;;) {
      await sleep(2000);
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
        updateJob(jobId, { state: 'installing' });
        try {
          const result = await installJob(server.uuid, jobId, path);
          updateJob(jobId, { state: 'installed', fileName: result.fileName });
          addToast(`Installed ${result.files?.join(', ') || result.fileName}`, 'success');
          loadInstalled();
        } catch (err) {
          updateJob(jobId, { state: 'failed', error: httpErrorToHuman(err) });
          addToast(httpErrorToHuman(err), 'error');
        }
        return;
      }
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
    });
    setJobs((prev) => [{ id: jobId, workshopId, state, title }, ...prev]);
    void pollJob(jobId, path);
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
  }, [mode, preset?.appId, searchQuery, searchSort, searchFileType, selectedTags.join('|'), config?.steamSearchAvailable]);

  const toggleTag = (tag: string) => {
    setSelectedTags((prev) => (prev.includes(tag) ? prev.filter((item) => item !== tag) : [...prev, tag]));
  };

  const clearSearchFilters = () => {
    setSearchQuery('');
    setSearchSort('popular');
    setSearchFileType('item');
    setSelectedTags([]);
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
      });
      const path = installPath.trim();
      const rows = result.jobs.map((job, index) => ({
        id: (job as any).jobId ?? (job as any).job_id,
        workshopId: collectionPreview.children[index]?.publishedFileId ?? 0,
        title: collectionPreview.children[index]?.title,
        state: job.state,
      }));
      setJobs((prev) => [...rows, ...prev]);
      for (const row of rows) {
        if (row.id) void pollJob(row.id, path);
      }
      addToast(`Queued ${rows.length} collection items`, 'success');
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
      setJobs((prev) => prev.filter((j) => j.id !== job.id));
    } catch (err) {
      addToast(httpErrorToHuman(err), 'error');
    }
  };

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

  const renderWorkshopCard = (item: WorkshopSearchItem, action: 'install' | 'collection' | 'none' = 'install') => (
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
          <Button size='xs' leftSection={<FontAwesomeIcon icon={faDownload} />} onClick={() => void startAndPoll(item.publishedFileId, item.title)}>
            Install
          </Button>
        ) : action === 'collection' ? (
          <Button size='xs' leftSection={<FontAwesomeIcon icon={faSearch} />} onClick={() => void previewCollectionId(item.publishedFileId)}>
            Preview
          </Button>
        ) : null}
      </Group>
    </Card>
  );

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
                    if (config?.presets[idx]) setInstallPath(config.presets[idx].installPath);
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
                  </Group>
                  <Group justify='space-between' align='end'>
                    <SegmentedControl
                      value={searchFileType}
                      onChange={(v) => {
                        setSearchFileType(v as WorkshopFileType);
                        setSelectedTags([]);
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

        {jobs.length > 0 ? (
          <Card withBorder radius='md' padding='lg'>
            <Title order={4} mb='sm'>Recent downloads</Title>
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
                {jobs.map((job) => (
                  <Table.Tr key={job.id}>
                    <Table.Td>{job.title ?? (job.workshopId || job.id)}</Table.Td>
                    <Table.Td>{job.fileName ?? '-'}</Table.Td>
                    <Table.Td>
                      <Badge color={stateColor(job.state)}>{job.state}</Badge>
                      {job.error ? <Text size='xs' c='red'>{job.error}</Text> : null}
                    </Table.Td>
                    <Table.Td align='right'>
                      <ServerCan action='workshop.install'>
                        <ActionIcon color='red' variant='subtle' aria-label='Remove recent download' title='Remove recent download' onClick={() => handleDeleteJob(job)}>
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

        <Card withBorder radius='md' padding='lg'>
          <Group justify='space-between' mb='sm'>
            <Title order={4}>Installed content</Title>
            <Button variant='subtle' leftSection={<FontAwesomeIcon icon={faRotate} />} onClick={loadInstalled}>Refresh</Button>
          </Group>
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
