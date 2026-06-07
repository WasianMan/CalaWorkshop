import { faDownload, faRotate, faTrash, faPlus } from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import {
  ActionIcon,
  Alert,
  Badge,
  Button,
  Card,
  Group,
  Loader,
  Select,
  Stack,
  Switch,
  Table,
  Text,
  TextInput,
  Title,
} from '@mantine/core';
import { useEffect, useMemo, useState } from 'react';
import { httpErrorToHuman } from '@/api/axios.ts';
import deleteDownload from '../api/deleteDownload.ts';
import deleteInstalled from '../api/deleteInstalled.ts';
import getConfig, { type WorkshopConfig } from '../api/getConfig.ts';
import getJob from '../api/getJob.ts';
import importInstalled from '../api/importInstalled.ts';
import installJob from '../api/installJob.ts';
import listDownloads from '../api/listDownloads.ts';
import listInstalled, { type InstalledEntry } from '../api/listInstalled.ts';
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

export default function WorkshopPage() {
  const server = useServerStore((s) => s.server);
  const { addToast } = useToast();

  const [config, setConfig] = useState<WorkshopConfig | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [presetIndex, setPresetIndex] = useState(0);
  const [installPath, setInstallPath] = useState('');
  const [workshopInput, setWorkshopInput] = useState('');
  const [archive, setArchive] = useState(false);
  const [account, setAccount] = useState<string | null>(null);
  const [accounts, setAccounts] = useState<string[]>([]);
  const [submitting, setSubmitting] = useState(false);
  const [jobs, setJobs] = useState<JobRow[]>([]);
  const [installed, setInstalled] = useState<InstalledEntry[]>([]);
  const [installedLoading, setInstalledLoading] = useState(false);

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
        if (cfg.presets.length > 0) {
          // Preselect the preset matching the auto-detected app id, else the first.
          const detectedIdx =
            cfg.detectedAppId != null ? cfg.presets.findIndex((p) => p.appId === cfg.detectedAppId) : -1;
          const idx = detectedIdx >= 0 ? detectedIdx : 0;
          setPresetIndex(idx);
          setInstallPath(cfg.presets[idx].installPath);
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

  const preset = useMemo(() => config?.presets[presetIndex], [config, presetIndex]);
  const auth = preset?.auth ?? 'default';
  const accountRequired = auth === 'account' || (auth === 'default' && config?.defaultAnonymous === false);

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

  const handleInstall = async () => {
    if (!preset) return;
    const workshopId = parseWorkshopId(workshopInput);
    if (!workshopId) {
      addToast('Could not read a Workshop ID from that input', 'error');
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

    setSubmitting(true);
    try {
      const { jobId, state } = await startDownload(server.uuid, {
        appId: preset.appId,
        workshopId,
        account: config?.canLinkSteam ? account : null,
        archive,
      });
      setJobs((prev) => [{ id: jobId, workshopId, state }, ...prev]);
      setWorkshopInput('');
      void pollJob(jobId, path);
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
              <Title order={4}>Install a Workshop item</Title>
              <Group grow align='end'>
                <Select
                  label='Game'
                  data={(config?.presets ?? []).map((p, i) => ({ value: String(i), label: p.name }))}
                  value={String(presetIndex)}
                  onChange={(v) => {
                    const idx = Number(v ?? 0);
                    setPresetIndex(idx);
                    if (config?.presets[idx]) setInstallPath(config.presets[idx].installPath);
                  }}
                />
                <TextInput label='Install path' value={installPath} onChange={(e) => setInstallPath(e.currentTarget.value)} />
              </Group>
              {config?.detectedAppId != null && preset?.appId === config.detectedAppId ? (
                <Text size='xs' c='dimmed'>
                  Auto-selected from this server&apos;s game ({config.detectedAppIdConfidence} confidence). Change it above if needed.
                </Text>
              ) : null}
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
                <Button leftSection={<FontAwesomeIcon icon={faDownload} />} loading={submitting} onClick={handleInstall} disabled={!config?.helperConfigured}>
                  Download &amp; install
                </Button>
              </Group>
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
                    <Table.Td>{job.title ?? job.workshopId}</Table.Td>
                    <Table.Td>{job.fileName ?? '-'}</Table.Td>
                    <Table.Td>
                      <Badge color={stateColor(job.state)}>{job.state}</Badge>
                      {job.error ? <Text size='xs' c='red'>{job.error}</Text> : null}
                    </Table.Td>
                    <Table.Td align='right'>
                      <ServerCan action='workshop.install'>
                        <ActionIcon
                          color='red'
                          variant='subtle'
                          aria-label='Remove recent download'
                          title='Remove recent download'
                          onClick={() => handleDeleteJob(job)}
                        >
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
