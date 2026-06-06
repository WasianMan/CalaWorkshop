import { faTrash } from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import {
  ActionIcon,
  Alert,
  Badge,
  Button,
  Card,
  Group,
  PasswordInput,
  Stack,
  Table,
  Text,
  TextInput,
  Title,
} from '@mantine/core';
import { useEffect, useState } from 'react';
import { httpErrorToHuman } from '@/api/axios.ts';
import deleteAccount from '@/api/steam/deleteAccount.ts';
import listAccounts, { type SteamAccount } from '@/api/steam/listAccounts.ts';
import loginAccount from '@/api/steam/loginAccount.ts';
import AccountContentContainer from '@/elements/containers/AccountContentContainer.tsx';
import { useToast } from '@/providers/ToastProvider.tsx';

export default function SteamLinkPage() {
  const { addToast } = useToast();

  const [accounts, setAccounts] = useState<SteamAccount[]>([]);
  const [label, setLabel] = useState('');
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [guardCode, setGuardCode] = useState('');
  const [needsGuard, setNeedsGuard] = useState(false);
  const [submitting, setSubmitting] = useState(false);

  const refresh = () => {
    listAccounts()
      .then(setAccounts)
      .catch(() => setAccounts([]));
  };

  // biome-ignore lint/correctness/useExhaustiveDependencies: load once on mount
  useEffect(() => {
    refresh();
  }, []);

  const handleLogin = async () => {
    if (!label.trim() || !username.trim()) {
      addToast('Label and username are required', 'error');
      return;
    }
    setSubmitting(true);
    try {
      const result = await loginAccount({
        label: label.trim(),
        username: username.trim(),
        password,
        guardCode: needsGuard ? guardCode.trim() : null,
      });
      if (result.state === 'ok') {
        addToast(`Linked ${label}`, 'success');
        setNeedsGuard(false);
        setPassword('');
        setGuardCode('');
        refresh();
      } else if (result.state === 'needs_guard') {
        setNeedsGuard(true);
      }
    } catch (err: any) {
      // The helper returns 409 when a Steam Guard code is required.
      if (err?.response?.status === 409) {
        setNeedsGuard(true);
        addToast('Enter the Steam Guard code and submit again', 'error');
      } else {
        addToast(httpErrorToHuman(err), 'error');
      }
    } finally {
      setSubmitting(false);
    }
  };

  const handleDelete = async (accountLabel: string) => {
    try {
      await deleteAccount(accountLabel);
      addToast(`Unlinked ${accountLabel}`, 'success');
      refresh();
    } catch (err) {
      addToast(httpErrorToHuman(err), 'error');
    }
  };

  return (
    <AccountContentContainer title='Steam Link'>
      <Stack gap='md'>
        <Alert color='blue' title='How Steam linking works'>
          Anonymous downloads work for some games, but many (including Left 4 Dead 2) require an
          account that owns the game. Linking logs the helper into your Steam account once and
          caches the session — your password is not stored long-term. A fresh login may ask for a
          Steam Guard code.
        </Alert>

        <Card withBorder radius='md' padding='lg'>
          <Title order={4} mb='sm'>Link a Steam account</Title>
          <Stack gap='sm'>
            <Group grow>
              <TextInput
                label='Label'
                placeholder='e.g. main'
                value={label}
                onChange={(e) => setLabel(e.currentTarget.value)}
              />
              <TextInput
                label='Steam username'
                value={username}
                onChange={(e) => setUsername(e.currentTarget.value)}
              />
            </Group>
            <PasswordInput
              label='Steam password'
              value={password}
              onChange={(e) => setPassword(e.currentTarget.value)}
            />
            {needsGuard ? (
              <TextInput
                label='Steam Guard code'
                description='Sent to your email / authenticator'
                value={guardCode}
                onChange={(e) => setGuardCode(e.currentTarget.value)}
              />
            ) : null}
            <Group>
              <Button loading={submitting} onClick={handleLogin}>
                {needsGuard ? 'Submit code' : 'Link account'}
              </Button>
            </Group>
          </Stack>
        </Card>

        <Card withBorder radius='md' padding='lg'>
          <Title order={4} mb='sm'>Linked accounts</Title>
          {accounts.length === 0 ? (
            <Text c='dimmed' size='sm'>No linked accounts yet.</Text>
          ) : (
            <Table>
              <Table.Thead>
                <Table.Tr>
                  <Table.Th>Label</Table.Th>
                  <Table.Th>Session</Table.Th>
                  <Table.Th />
                </Table.Tr>
              </Table.Thead>
              <Table.Tbody>
                {accounts.map((acc) => (
                  <Table.Tr key={acc.label}>
                    <Table.Td>{acc.label}</Table.Td>
                    <Table.Td>
                      <Badge color={acc.valid ? 'green' : 'gray'}>
                        {acc.valid ? 'linked' : 'unknown'}
                      </Badge>
                    </Table.Td>
                    <Table.Td align='right'>
                      <ActionIcon color='red' variant='subtle' onClick={() => handleDelete(acc.label)}>
                        <FontAwesomeIcon icon={faTrash} />
                      </ActionIcon>
                    </Table.Td>
                  </Table.Tr>
                ))}
              </Table.Tbody>
            </Table>
          )}
        </Card>
      </Stack>
    </AccountContentContainer>
  );
}
