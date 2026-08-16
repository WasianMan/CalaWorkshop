import { faTrash } from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import {
  ActionIcon,
  Alert,
  Badge,
  Button,
  Card,
  Center,
  Group,
  Image,
  Loader,
  PasswordInput,
  SegmentedControl,
  Stack,
  Table,
  Text,
  TextInput,
  Title,
} from '@mantine/core';
import { useEffect, useRef, useState } from 'react';
import { httpErrorToHuman } from '@/api/axios.ts';
import deleteAccount from '../api/steam/deleteAccount.ts';
import listAccounts, { type SteamAccount } from '../api/steam/listAccounts.ts';
import {
  beginPasswordSession,
  beginQrSession,
  cancelLoginSession,
  getLoginSession,
  type LoginSession,
} from '../api/steam/loginSessions.ts';
import AccountContentContainer from '@/elements/containers/AccountContentContainer.tsx';
import { useToast } from '@/providers/ToastProvider.tsx';

const TERMINAL_STATES = new Set(['needs_guard', 'ok', 'failed']);
const POLL_INTERVAL_MS = 2500;

function statusLine(session: LoginSession): string {
  switch (session.state) {
    case 'running':
      return 'Contacting Steam…';
    case 'awaiting_qr':
      return 'Scan the QR code with the Steam Mobile app, then approve the sign-in.';
    case 'awaiting_mobile_confirmation':
      return 'Approve this sign-in in the Steam Mobile app on your phone. Waiting…';
    case 'verifying':
      return 'Login accepted — verifying the cached session…';
    default:
      return '';
  }
}

export default function SteamLinkPage() {
  const { addToast } = useToast();

  const [accounts, setAccounts] = useState<SteamAccount[]>([]);
  const [method, setMethod] = useState<'qr' | 'password'>('qr');
  const [label, setLabel] = useState('');
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [guardCode, setGuardCode] = useState('');
  const [submitting, setSubmitting] = useState(false);
  const [session, setSession] = useState<LoginSession | null>(null);
  // Label the active session was started with (inputs stay editable safely).
  const sessionLabelRef = useRef('');

  const refresh = () => {
    listAccounts()
      .then(setAccounts)
      .catch(() => setAccounts([]));
  };

  // biome-ignore lint/correctness/useExhaustiveDependencies: load once on mount
  useEffect(() => {
    refresh();
  }, []);

  const sessionActive = session !== null && !TERMINAL_STATES.has(session.state);
  const needsGuard = session?.state === 'needs_guard';

  // Poll the active session until it reaches a terminal state.
  // biome-ignore lint/correctness/useExhaustiveDependencies: poll keyed on session id/state
  useEffect(() => {
    if (!session || TERMINAL_STATES.has(session.state)) {
      return;
    }
    const id = session.id;
    const timer = setInterval(async () => {
      try {
        const next = await getLoginSession(id, sessionLabelRef.current);
        if (next.state === 'ok') {
          addToast(`Linked and verified ${sessionLabelRef.current}`, 'success');
          setSession(null);
          setPassword('');
          setGuardCode('');
          refresh();
          return;
        }
        setSession(next);
      } catch {
        // Session evaporated (helper restart / GC) — reset quietly.
        setSession(null);
        addToast('The login session ended unexpectedly — try again', 'error');
      }
    }, POLL_INTERVAL_MS);
    return () => clearInterval(timer);
  }, [session?.id, session?.state]);

  const startQr = async () => {
    if (!label.trim()) {
      addToast('Enter a label first', 'error');
      return;
    }
    setSubmitting(true);
    try {
      sessionLabelRef.current = label.trim();
      setSession(await beginQrSession(label.trim()));
    } catch (err) {
      addToast(httpErrorToHuman(err), 'error');
    } finally {
      setSubmitting(false);
    }
  };

  const startPassword = async () => {
    if (!label.trim() || !username.trim()) {
      addToast('Label and username are required', 'error');
      return;
    }
    setSubmitting(true);
    try {
      sessionLabelRef.current = label.trim();
      setSession(
        await beginPasswordSession({
          label: label.trim(),
          username: username.trim(),
          password,
          guardCode: guardCode.trim() ? guardCode.trim() : null,
        }),
      );
    } catch (err) {
      addToast(httpErrorToHuman(err), 'error');
    } finally {
      setSubmitting(false);
    }
  };

  const cancelSession = async () => {
    if (session) {
      cancelLoginSession(session.id, sessionLabelRef.current).catch(() => undefined);
    }
    setSession(null);
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

  const failed = session?.state === 'failed' ? session : null;

  return (
    <AccountContentContainer title='Steam Link'>
      <Stack gap='md'>
        <Alert color='blue' title='How Steam linking works'>
          Anonymous downloads work for some games, but many (including Left 4 Dead 2) require an
          account that owns the game. Linking signs the helper into your Steam account once and
          caches the session — your password is never stored. Accounts you link here are tied to
          your user and are not visible to other panel users. The QR method is the easiest: scan
          with the Steam Mobile app and approve, no password needed.
        </Alert>

        <Card withBorder radius='md' padding='lg'>
          <Title order={4} mb='sm'>
            Link a Steam account
          </Title>
          <Stack gap='sm'>
            <SegmentedControl
              value={method}
              onChange={(value) => {
                setMethod(value as 'qr' | 'password');
                if (session) cancelSession();
              }}
              disabled={sessionActive}
              data={[
                { label: 'QR code (Steam Mobile app)', value: 'qr' },
                { label: 'Password', value: 'password' },
              ]}
            />

            <TextInput
              label='Label'
              description='A name for this link, e.g. main'
              placeholder='e.g. main'
              value={label}
              disabled={sessionActive}
              onChange={(e) => setLabel(e.currentTarget.value)}
            />

            {method === 'password' ? (
              <>
                <Group grow>
                  <TextInput
                    label='Steam username'
                    value={username}
                    disabled={sessionActive}
                    onChange={(e) => setUsername(e.currentTarget.value)}
                  />
                  <PasswordInput
                    label='Steam password'
                    value={password}
                    disabled={sessionActive}
                    onChange={(e) => setPassword(e.currentTarget.value)}
                  />
                </Group>
                {needsGuard ? (
                  <TextInput
                    label='Steam Guard code'
                    description={
                      session?.guardHint === 'email'
                        ? 'Steam emailed a code to the address on the account — enter it here.'
                        : "Open the Steam Mobile app and enter the 5-character code from the Steam Guard tab (the rotating code), then submit again. Approving in the app instead also works — resubmit and approve when prompted."
                    }
                    value={guardCode}
                    onChange={(e) => setGuardCode(e.currentTarget.value)}
                  />
                ) : null}
              </>
            ) : null}

            {sessionActive && session ? (
              <Alert color={session.state === 'awaiting_mobile_confirmation' ? 'yellow' : 'blue'}>
                <Stack gap='xs'>
                  <Group gap='xs'>
                    <Loader size='xs' />
                    <Text size='sm'>{statusLine(session)}</Text>
                  </Group>
                  {session.state === 'awaiting_qr' && session.qrSvg ? (
                    <Center>
                      <Image
                        src={`data:image/svg+xml;base64,${btoa(session.qrSvg)}`}
                        alt='Steam sign-in QR code'
                        w={220}
                        h={220}
                        fit='contain'
                      />
                    </Center>
                  ) : null}
                  {session.state === 'awaiting_mobile_confirmation' ? (
                    <Text size='xs' c='dimmed'>
                      Keep this page open — the approval is usually instant once you tap. If the
                      app only shows a code instead of an approval, cancel and use the code field.
                    </Text>
                  ) : null}
                </Stack>
              </Alert>
            ) : null}

            {failed ? (
              <Alert color='red' title='Login failed'>
                <Text size='sm'>{failed.error ?? 'Unknown error'}</Text>
                {failed.errorKind === 'qr_unsupported' ? (
                  <Text size='sm' mt='xs'>
                    Switch to the <b>Password</b> method above to finish linking this account.
                  </Text>
                ) : null}
              </Alert>
            ) : null}

            <Group>
              {sessionActive ? (
                <Button variant='default' onClick={cancelSession}>
                  Cancel
                </Button>
              ) : method === 'qr' ? (
                <Button loading={submitting} onClick={startQr}>
                  Show QR code
                </Button>
              ) : (
                <Button loading={submitting} onClick={startPassword}>
                  {needsGuard ? 'Submit code' : 'Link account'}
                </Button>
              )}
            </Group>
          </Stack>
        </Card>

        <Card withBorder radius='md' padding='lg'>
          <Title order={4} mb='sm'>
            Linked accounts
          </Title>
          {accounts.length === 0 ? (
            <Text c='dimmed' size='sm'>
              No linked accounts yet.
            </Text>
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
                      <ActionIcon
                        color='red'
                        variant='subtle'
                        onClick={() => handleDelete(acc.label)}
                      >
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
