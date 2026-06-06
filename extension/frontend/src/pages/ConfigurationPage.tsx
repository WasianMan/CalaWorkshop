import { faPlus, faTrash } from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import {
  ActionIcon,
  Button,
  Card,
  Group,
  NumberInput,
  PasswordInput,
  Stack,
  Switch,
  Text,
  TextInput,
  Title,
} from '@mantine/core';
import { useEffect, useState } from 'react';
import { httpErrorToHuman } from '@/api/axios.ts';
import getSettings from '../api/admin/getSettings.ts';
import updateSettings from '../api/admin/updateSettings.ts';
import type { GamePreset } from '../api/getConfig.ts';
import { useToast } from '@/providers/ToastProvider.tsx';

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
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    getSettings()
      .then((s) => {
        setHelperUrl(s.helperUrl);
        setHelperTokenSet(s.helperTokenSet);
        setSteamApiKeySet(s.steamApiKeySet);
        setDefaultAnonymous(s.defaultAnonymous);
        setPresets(s.gamePresets);
        setLoaded(true);
      })
      .catch((err) => addToast(httpErrorToHuman(err), 'error'));
    // biome-ignore lint/correctness/useExhaustiveDependencies: load once on mount
  }, []);

  const updatePreset = (index: number, patch: Partial<GamePreset>) =>
    setPresets((prev) => prev.map((p, i) => (i === index ? { ...p, ...patch } : p)));

  const handleSave = async () => {
    setSaving(true);
    try {
      await updateSettings({
        helperUrl,
        // Only send secrets when the admin typed something, so we don't clear them on save.
        helperToken: helperToken !== '' ? helperToken : undefined,
        steamApiKey: steamApiKey !== '' ? steamApiKey : undefined,
        defaultAnonymous,
        gamePresets: presets,
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
                ? 'A key is set — leave blank to keep it. Used for search/metadata only.'
                : 'Optional. Used for search/metadata only, never for downloads.'
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
          <Title order={4}>Game presets</Title>
          <Button
            variant='subtle'
            leftSection={<FontAwesomeIcon icon={faPlus} />}
            onClick={() =>
              setPresets((prev) => [...prev, { appId: 0, name: '', installPath: '' }])
            }
          >
            Add preset
          </Button>
        </Group>
        <Stack gap='sm'>
          {presets.map((preset, index) => (
            <Group key={index} grow align='end'>
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
              <ActionIcon
                color='red'
                variant='subtle'
                onClick={() => setPresets((prev) => prev.filter((_, i) => i !== index))}
              >
                <FontAwesomeIcon icon={faTrash} />
              </ActionIcon>
            </Group>
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
