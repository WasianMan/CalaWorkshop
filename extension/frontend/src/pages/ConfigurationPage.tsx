import { faPlus, faTrash } from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import {
  ActionIcon,
  Alert,
  Anchor,
  Badge,
  Button,
  Card,
  Group,
  NumberInput,
  PasswordInput,
  Stack,
  Switch,
  Text,
  Textarea,
  TextInput,
  Title,
} from '@mantine/core';
import { useEffect, useState } from 'react';
import { httpErrorToHuman } from '@/api/axios.ts';
import getDiagnostics, { type Diagnostics } from '../api/admin/getDiagnostics.ts';
import getSettings from '../api/admin/getSettings.ts';
import updateSettings from '../api/admin/updateSettings.ts';
import type { GamePreset } from '../api/getConfig.ts';
import { useToast } from '@/providers/ToastProvider.tsx';

const AUTHS = ['default', 'anonymous', 'account'];
const POST_INSTALLS = ['none', 'extract'];

/** Render a preset's advanced rule fields as the editable JSON document. */
function advancedJson(p: GamePreset): string {
  return JSON.stringify(
    {
      auth: p.auth ?? 'default',
      match: p.match ?? [],
      generatedFiles: p.generatedFiles ?? [],
      scan: p.scan ?? [],
      postInstall: p.postInstall ?? 'none',
    },
    null,
    2,
  );
}

export default function ConfigurationPage() {
  const { addToast } = useToast();

  const [loaded, setLoaded] = useState(false);
  const [helperUrl, setHelperUrl] = useState('');
  const [helperToken, setHelperToken] = useState('');
  const [helperTokenSet, setHelperTokenSet] = useState(false);
  const [steamApiKey, setSteamApiKey] = useState('');
  const [steamApiKeySet, setSteamApiKeySet] = useState(false);
  const [defaultAnonymous, setDefaultAnonymous] = useState(true);
  const [presets, setPresets] = useState<GamePreset[]>([]);
  // Per-preset advanced rule, edited as raw JSON, aligned with `presets` by index.
  const [advanced, setAdvanced] = useState<string[]>([]);
  const [openAdvanced, setOpenAdvanced] = useState<boolean[]>([]);
  const [saving, setSaving] = useState(false);
  const [diagnostics, setDiagnostics] = useState<Diagnostics | null>(null);
  const [diagnosticsLoading, setDiagnosticsLoading] = useState(false);

  useEffect(() => {
    getSettings()
      .then((s) => {
        setHelperUrl(s.helperUrl);
        setHelperTokenSet(s.helperTokenSet);
        setSteamApiKeySet(s.steamApiKeySet);
        setDefaultAnonymous(s.defaultAnonymous);
        setPresets(s.gamePresets);
        setAdvanced(s.gamePresets.map(advancedJson));
        setOpenAdvanced(s.gamePresets.map(() => false));
        setLoaded(true);
      })
      .catch((err) => addToast(httpErrorToHuman(err), 'error'));
    // biome-ignore lint/correctness/useExhaustiveDependencies: load once on mount
  }, []);

  const updatePreset = (index: number, patch: Partial<GamePreset>) =>
    setPresets((prev) => prev.map((p, i) => (i === index ? { ...p, ...patch } : p)));

  const addPreset = () => {
    setPresets((prev) => [...prev, { appId: 0, name: '', installPath: '' }]);
    setAdvanced((prev) => [...prev, advancedJson({ appId: 0, name: '', installPath: '' })]);
    setOpenAdvanced((prev) => [...prev, false]);
  };

  const removePreset = (index: number) => {
    setPresets((prev) => prev.filter((_, i) => i !== index));
    setAdvanced((prev) => prev.filter((_, i) => i !== index));
    setOpenAdvanced((prev) => prev.filter((_, i) => i !== index));
  };

  const handleSave = async () => {
    // Merge each preset's advanced JSON back into the preset, validating shape.
    const merged: GamePreset[] = [];
    for (let i = 0; i < presets.length; i++) {
      const label = presets[i].name || `#${i + 1}`;
      let adv: {
        auth?: string;
        match?: unknown;
        generatedFiles?: unknown;
        scan?: unknown;
        postInstall?: string;
      };
      try {
        adv = JSON.parse(advanced[i] || '{}');
      } catch {
        addToast(`Preset ${label}: advanced JSON is not valid`, 'error');
        return;
      }
      const auth = adv.auth ?? 'default';
      if (!AUTHS.includes(auth)) {
        addToast(`Preset ${label}: auth must be one of ${AUTHS.join(', ')}`, 'error');
        return;
      }
      const postInstall = adv.postInstall ?? 'none';
      if (!POST_INSTALLS.includes(postInstall)) {
        addToast(`Preset ${label}: postInstall must be one of ${POST_INSTALLS.join(', ')}`, 'error');
        return;
      }
      const rawMatch = Array.isArray(adv.match) ? adv.match : [];
      const match: { glob: string; rename?: string }[] = [];
      for (const m of rawMatch as Array<{ glob?: unknown; rename?: unknown }>) {
        if (typeof m?.glob !== 'string' || !m.glob.trim()) {
          addToast(`Preset ${label}: every match entry needs a non-empty "glob"`, 'error');
          return;
        }
        match.push({ glob: m.glob, ...(typeof m.rename === 'string' ? { rename: m.rename } : {}) });
      }
      const rawGeneratedFiles = Array.isArray(adv.generatedFiles) ? adv.generatedFiles : [];
      const generatedFiles: { path: string; content: string }[] = [];
      for (const g of rawGeneratedFiles as Array<{ path?: unknown; content?: unknown }>) {
        if (typeof g?.path !== 'string' || !g.path.trim() || typeof g.content !== 'string') {
          addToast(`Preset ${label}: every generatedFiles entry needs "path" and "content" strings`, 'error');
          return;
        }
        generatedFiles.push({ path: g.path, content: g.content });
      }
      const rawScan = Array.isArray(adv.scan) ? adv.scan : [];
      const scan: { path: string; extensions?: string[]; glob?: string }[] = [];
      for (const s of rawScan as Array<{ path?: unknown; extensions?: unknown; glob?: unknown }>) {
        if (typeof s?.path !== 'string' || !s.path.trim()) {
          addToast(`Preset ${label}: every scan entry needs a non-empty "path"`, 'error');
          return;
        }
        const extensions = Array.isArray(s.extensions)
          ? s.extensions.filter((ext): ext is string => typeof ext === 'string')
          : [];
        scan.push({
          path: s.path,
          extensions,
          ...(typeof s.glob === 'string' && s.glob ? { glob: s.glob } : {}),
        });
      }
      merged.push({
        ...presets[i],
        auth: auth as GamePreset['auth'],
        match,
        generatedFiles,
        scan,
        postInstall: postInstall as GamePreset['postInstall'],
      });
    }

    setSaving(true);
    try {
      await updateSettings({
        helperUrl,
        // Only send secrets when the admin typed something, so we don't clear them on save.
        helperToken: helperToken !== '' ? helperToken : undefined,
        steamApiKey: steamApiKey !== '' ? steamApiKey : undefined,
        defaultAnonymous,
        gamePresets: merged,
      });
      addToast('Settings saved', 'success');
      if (helperToken !== '') setHelperTokenSet(true);
      if (steamApiKey !== '') setSteamApiKeySet(true);
      setHelperToken('');
      setSteamApiKey('');
    } catch (err) {
      addToast(httpErrorToHuman(err), 'error');
    } finally {
      setSaving(false);
    }
  };

  const runDiagnostics = async () => {
    setDiagnosticsLoading(true);
    try {
      setDiagnostics(await getDiagnostics());
    } catch (err) {
      addToast(httpErrorToHuman(err), 'error');
    } finally {
      setDiagnosticsLoading(false);
    }
  };

  if (!loaded) {
    return <Text c='dimmed'>Loading…</Text>;
  }

  return (
    <Stack gap='md'>
      <Card withBorder radius='md' padding='lg'>
        <Title order={4} mb='sm'>Helper connection</Title>
        <Stack gap='sm'>
          <TextInput
            label='Helper URL'
            description='Reachable by the panel and by Wings (the AIO compose service name)'
            value={helperUrl}
            onChange={(e) => setHelperUrl(e.currentTarget.value)}
          />
          <PasswordInput
            label='Helper token'
            description={helperTokenSet ? 'A token is set — leave blank to keep it' : 'Shared bearer token'}
            placeholder={helperTokenSet ? '••••••••' : ''}
            value={helperToken}
            onChange={(e) => setHelperToken(e.currentTarget.value)}
          />
        </Stack>
      </Card>

      <Card withBorder radius='md' padding='lg'>
        <Title order={4} mb='sm'>Steam</Title>
        <Stack gap='sm'>
          <PasswordInput
            label='Steam Web API key'
            description={
              steamApiKeySet
                ? 'A key is set - leave blank to keep it. SteamCMD handles downloads; this key is only for names, previews, and search metadata.'
                : 'Optional. SteamCMD handles downloads; this key is only for names, previews, and search metadata.'
            }
            placeholder={steamApiKeySet ? '••••••••' : ''}
            value={steamApiKey}
            onChange={(e) => setSteamApiKey(e.currentTarget.value)}
          />
          <Switch
            label='Default to anonymous downloads'
            checked={defaultAnonymous}
            onChange={(e) => setDefaultAnonymous(e.currentTarget.checked)}
          />
        </Stack>
      </Card>

      <Card withBorder radius='md' padding='lg'>
        <Group justify='space-between' mb='sm'>
          <Title order={4}>Diagnostics</Title>
          <Button variant='subtle' loading={diagnosticsLoading} onClick={runDiagnostics}>
            Run checks
          </Button>
        </Group>
        {diagnostics ? (
          <Stack gap='xs'>
            {(['helper', 'steamcmd'] as const).map((key) => (
              <Alert key={key} color={diagnostics[key].ok ? 'green' : 'red'} title={
                <Group gap='xs'>
                  <span>{key === 'helper' ? 'Helper' : 'SteamCMD'}</span>
                  <Badge color={diagnostics[key].ok ? 'green' : 'red'}>
                    {diagnostics[key].ok ? 'ok' : 'failed'}
                  </Badge>
                </Group>
              }>
                {diagnostics[key].message ?? diagnostics[key].error ?? 'No details returned'}
              </Alert>
            ))}
          </Stack>
        ) : (
          <Text c='dimmed' size='sm'>Run checks after saving helper settings.</Text>
        )}
      </Card>

      <Card withBorder radius='md' padding='lg'>
        <Group justify='space-between' mb='sm'>
          <Title order={4}>Game presets</Title>
          <Button variant='subtle' leftSection={<FontAwesomeIcon icon={faPlus} />} onClick={addPreset}>
            Add preset
          </Button>
        </Group>
        <Stack gap='lg'>
          {presets.map((preset, index) => (
            <Stack key={index} gap='xs'>
              <Group grow align='end'>
                <NumberInput
                  label='App ID'
                  value={preset.appId}
                  onChange={(v) => updatePreset(index, { appId: Number(v) || 0 })}
                />
                <TextInput
                  label='Name'
                  value={preset.name}
                  onChange={(e) => updatePreset(index, { name: e.currentTarget.value })}
                />
                <TextInput
                  label='Install path'
                  value={preset.installPath}
                  onChange={(e) => updatePreset(index, { installPath: e.currentTarget.value })}
                />
                <ActionIcon color='red' variant='subtle' onClick={() => removePreset(index)}>
                  <FontAwesomeIcon icon={faTrash} />
                </ActionIcon>
              </Group>
              <Anchor
                size='xs'
                onClick={() =>
                  setOpenAdvanced((prev) => prev.map((o, i) => (i === index ? !o : o)))
                }
              >
                {openAdvanced[index] ? '▾' : '▸'} Advanced (JSON)
              </Anchor>
              {openAdvanced[index] ? (
                <Textarea
                  label='Install rule'
                  description='auth: default | anonymous | account · match: [{ "glob", "rename"? }] · generatedFiles: [{ "path", "content" }] · scan: [{ "path", "extensions"? }] · postInstall: none | extract. Tokens: {workshop_id} {app_id} {ext} {basename} {title_slug}'
                  value={advanced[index] ?? ''}
                  onChange={(v) =>
                    setAdvanced((prev) => prev.map((t, i) => (i === index ? v.currentTarget.value : t)))
                  }
                  autosize
                  minRows={5}
                  styles={{ input: { fontFamily: 'monospace' } }}
                />
              ) : null}
            </Stack>
          ))}
        </Stack>
      </Card>

      <Group>
        <Button loading={saving} onClick={handleSave}>
          Save
        </Button>
      </Group>
    </Stack>
  );
}
