#!/usr/bin/env node

import { createServer } from 'node:http';
import { randomBytes } from 'node:crypto';
import { execFile } from 'node:child_process';
import { platform } from 'node:os';
import { readFile, writeFile } from 'node:fs/promises';

const GOOGLE_AUTH_URL = 'https://accounts.google.com/o/oauth2/auth';
const GOOGLE_TOKEN_URL = 'https://oauth2.googleapis.com/token';
const GOOGLE_USERINFO_URL = 'https://www.googleapis.com/oauth2/v3/userinfo';
const GMAIL_API_BASE_URL = 'https://gmail.googleapis.com/gmail/v1/users';
const CALLBACK_TIMEOUT_MS = 5 * 60_000;
const DEFAULT_SCOPES = [
  'openid',
  'https://www.googleapis.com/auth/userinfo.email',
  'https://www.googleapis.com/auth/gmail.modify',
];

async function main() {
  const args = parseArgs(process.argv.slice(2));

  if (args.help) {
    printHelp();
    return;
  }

  const mode = normalizeMode(args.mode ?? 'watch');
  const oauthClient = args.credentials ? await loadOAuthClient(args.credentials) : null;
  const chosenRedirectUri = oauthClient
    ? chooseRedirectUri(oauthClient.redirectUris, args.redirectUri)
    : undefined;

  if (args.printAuthUrl) {
    if (!oauthClient || !chosenRedirectUri) {
      throw new Error('--credentials is required with --print-auth-url');
    }

    const authUrl = buildAuthUrl({
      clientId: oauthClient.clientId,
      redirectUri: chosenRedirectUri,
      scopes: DEFAULT_SCOPES,
      state: 'manual-test',
    });

    process.stdout.write(
      [
        'Open this URL in a browser and authenticate as the mailbox you want to watch.',
        `Target mailbox: ${args.email ?? 'me'}`,
        `Redirect URI: ${chosenRedirectUri}`,
        authUrl,
      ].join('\n') + '\n',
    );
    return;
  }

  const tokens = args.tokens
    ? await loadSavedTokens(args.tokens)
    : args.callbackUrl
      ? await exchangeAuthorizationCode({
          clientId: (oauthClient ?? await loadOAuthClient(requireString(args, 'credentials'))).clientId,
          clientSecret: (oauthClient ?? await loadOAuthClient(requireString(args, 'credentials'))).clientSecret,
          redirectUri: chosenRedirectUri ?? chooseRedirectUri(
            (oauthClient ?? await loadOAuthClient(requireString(args, 'credentials'))).redirectUris,
            args.redirectUri,
          ),
          code: extractCodeFromCallbackUrl(args.callbackUrl),
        })
    : await runOAuthFlow(
        oauthClient ?? await loadOAuthClient(requireString(args, 'credentials')),
        chosenRedirectUri,
      );

  const userinfo = await fetchUserinfo(tokens.accessToken);

  if (args.saveTokens) {
    await saveTokens(args.saveTokens, {
      clientId: oauthClient?.clientId ?? tokens.clientId,
      clientSecret: oauthClient?.clientSecret ?? tokens.clientSecret,
    }, tokens);
  }

  if (mode === 'stop') {
    await stopWatch(tokens.accessToken, args.email ?? 'me');
    process.stdout.write(
      [
        'Watch stopped successfully.',
        `Authenticated account: ${userinfo.email}`,
      ].join('\n') + '\n',
    );
    return;
  }

  const topicName = requireString(args, 'topic');
  const labelIds = parseCsv(args.labels);
  const allowedSenders = parseCsv(args.allowedSenders);

  if (mode === 'replyWindow') {
    const parsedTopic = parseTopicName(topicName);
    const subscriptionName = typeof args.subscription === 'string' && args.subscription.trim() !== ''
      ? args.subscription.trim()
      : `${parsedTopic.topicId}-autoreply`;
    const timeoutSeconds = parsePositiveInteger(args.timeoutSeconds, 600, '--timeout-seconds');
    const pollIntervalMs = parsePositiveInteger(args.pollIntervalMs, 4_000, '--poll-interval-ms');
    const fixedReplyBody = typeof args.replyBody === 'string' && args.replyBody.trim() !== ''
      ? args.replyBody
      : null;

    if (allowedSenders.length === 0) {
      throw new Error('--allowed-senders is required for reply-window mode');
    }

    await ensurePubsubResources({
      projectId: parsedTopic.projectId,
      topicId: parsedTopic.topicId,
      subscriptionName,
    });

    const watchResult = await createWatch(tokens.accessToken, {
      userId: args.email ?? 'me',
      topicName,
      labelIds,
      labelFilterBehavior: labelIds.length > 0 ? (args.labelFilterBehavior ?? 'INCLUDE') : undefined,
    });

    process.stdout.write(
      [
        'Reply window listener armed.',
        `Authenticated account: ${userinfo.email}`,
        `Topic: ${topicName}`,
        `Subscription: projects/${parsedTopic.projectId}/subscriptions/${subscriptionName}`,
        `Baseline history ID: ${watchResult.historyId}`,
        `Watch expiration: ${new Date(Number(watchResult.expiration)).toISOString()}`,
        `Allowed senders: ${allowedSenders.join(', ')}`,
        fixedReplyBody ? 'Reply mode: fixed body' : 'Reply mode: generated replies',
        `Polling timeout: ${timeoutSeconds}s`,
        'Send the test email now.',
      ].join('\n') + '\n',
    );

    let handled;
    let failure = null;
    try {
      handled = await watchForIncomingMessagesAndReply({
        accessToken: tokens.accessToken,
        selfEmail: userinfo.email,
        userId: args.email ?? 'me',
        projectId: parsedTopic.projectId,
        subscriptionName,
        timeoutSeconds,
        pollIntervalMs,
        baselineHistoryId: String(watchResult.historyId),
        allowedSenders,
        fixedReplyBody,
      });
    } catch (error) {
      failure = error;
    } finally {
      try {
        await stopWatch(tokens.accessToken, args.email ?? 'me');
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        process.stderr.write(`Failed to stop Gmail watch automatically: ${message}\n`);
      }
    }

    if (failure) {
      throw failure;
    }

    process.stdout.write(
      [
        `Reply window finished after ${handled.elapsedSeconds}s.`,
        `Replies sent: ${handled.repliedCount}`,
        `Processed messages: ${handled.processedCount}`,
      ].join('\n') + '\n',
    );
    return;
  }

  const result = await createWatch(tokens.accessToken, {
    userId: args.email ?? 'me',
    topicName,
    labelIds,
    labelFilterBehavior: args.labelFilterBehavior ?? (labelIds.length > 0 ? 'INCLUDE' : undefined),
  });

  const lines = [
    'Watch created successfully.',
    `Authenticated account: ${userinfo.email}`,
    `History ID: ${result.historyId}`,
    `Expiration: ${new Date(Number(result.expiration)).toISOString()}`,
    `Topic: ${topicName}`,
  ];

  if (labelIds.length > 0) {
    lines.push(`Labels: ${labelIds.join(', ')}`);
  }

  process.stdout.write(lines.join('\n') + '\n');
}

function parseArgs(argv) {
  const args = {};

  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];

    if (token === '--help' || token === '-h') {
      args.help = true;
      continue;
    }

    if (!token.startsWith('--')) {
      throw new Error(`Unexpected positional argument: ${token}`);
    }

    const [rawKey, inlineValue] = token.slice(2).split('=', 2);
    const key = normalizeArgKey(rawKey);
    const next = inlineValue ?? argv[index + 1];
    const expectsValue = !isBooleanFlag(key);

    if (!expectsValue) {
      args[key] = true;
      continue;
    }

    if (inlineValue === undefined) {
      if (!next || next.startsWith('--')) {
        throw new Error(`Missing value for --${rawKey}`);
      }
      index += 1;
    }

    args[key] = inlineValue ?? next;
  }

  return args;
}

function normalizeArgKey(key) {
  return key.replace(/-([a-z])/g, (_, char) => char.toUpperCase());
}

function isBooleanFlag(key) {
  return key === 'help' || key === 'printAuthUrl';
}

function requireString(args, key) {
  const value = args[key];
  if (typeof value !== 'string' || value.trim() === '') {
    throw new Error(`--${toKebabCase(key)} is required`);
  }
  return value;
}

function toKebabCase(value) {
  return value.replace(/[A-Z]/g, (char) => `-${char.toLowerCase()}`);
}

function normalizeMode(mode) {
  const normalized = String(mode).trim().toLowerCase();
  if (normalized === 'reply-once' || normalized === 'replyonce') return 'replyOnce';
  if (normalized === 'reply-window' || normalized === 'replywindow') return 'replyWindow';
  if (normalized === 'watch' || normalized === 'stop') return normalized;
  throw new Error(`Unsupported mode: ${mode}`);
}

async function loadOAuthClient(credentialsPath) {
  const raw = JSON.parse(await readFile(credentialsPath, 'utf8'));
  const client = raw.installed ?? raw.web;

  if (!client) {
    throw new Error('Credentials JSON must contain either an "installed" or "web" client');
  }

  if (typeof client.client_id !== 'string' || typeof client.client_secret !== 'string') {
    throw new Error('Credentials JSON is missing client_id or client_secret');
  }

  if (!Array.isArray(client.redirect_uris) || client.redirect_uris.length === 0) {
    throw new Error('Credentials JSON must contain at least one redirect URI');
  }

  return {
    clientId: client.client_id,
    clientSecret: client.client_secret,
    projectId: client.project_id,
    redirectUris: client.redirect_uris,
    clientType: raw.installed ? 'installed' : 'web',
  };
}

async function loadSavedTokens(tokensPath) {
  const raw = JSON.parse(await readFile(tokensPath, 'utf8'));
  const refreshToken = raw.refresh_token;

  if (typeof refreshToken !== 'string' || refreshToken.length === 0) {
    throw new Error(`Token file ${tokensPath} does not contain refresh_token`);
  }

  if (typeof raw.client_id !== 'string' || typeof raw.client_secret !== 'string') {
    throw new Error(`Token file ${tokensPath} does not contain client_id/client_secret`);
  }

  return exchangeRefreshToken({
    clientId: raw.client_id,
    clientSecret: raw.client_secret,
    refreshToken,
  });
}

async function runOAuthFlow(credentials, redirectUriOverride) {
  const redirectUri = redirectUriOverride ?? chooseRedirectUri(credentials.redirectUris);
  const url = new URL(redirectUri);

  if (url.protocol !== 'http:') {
    throw new Error(`Only http redirect URIs are supported for the local callback flow: ${redirectUri}`);
  }

  const state = randomBytes(16).toString('hex');
  const authUrl = buildAuthUrl({
    clientId: credentials.clientId,
    redirectUri,
    scopes: DEFAULT_SCOPES,
    state,
  });

  process.stderr.write(
    [
      `Opening browser for OAuth consent (${credentials.clientType} client, project ${credentials.projectId ?? 'unknown'}).`,
      `Redirect URI: ${redirectUri}`,
    ].join('\n') + '\n',
  );

  const code = await listenForOAuthCode({ authUrl, redirectUri, state });
  return exchangeAuthorizationCode({
    clientId: credentials.clientId,
    clientSecret: credentials.clientSecret,
    redirectUri,
    code,
  });
}

function chooseRedirectUri(registeredRedirectUris, redirectUriOverride) {
  if (redirectUriOverride) {
    if (!registeredRedirectUris.includes(redirectUriOverride)) {
      throw new Error(
        `Requested redirect URI is not registered in the OAuth client: ${redirectUriOverride}`,
      );
    }
    return redirectUriOverride;
  }

  const localhostCandidate = registeredRedirectUris.find((value) => {
    const url = new URL(value);
    return url.protocol === 'http:' && (url.hostname === 'localhost' || url.hostname === '127.0.0.1');
  });

  return localhostCandidate ?? registeredRedirectUris[0];
}

function extractCodeFromCallbackUrl(callbackUrl) {
  const url = new URL(callbackUrl);
  const code = url.searchParams.get('code');
  if (!code) {
    throw new Error('--callback-url does not contain a code parameter');
  }
  return code;
}

function buildAuthUrl({ clientId, redirectUri, scopes, state }) {
  const params = new URLSearchParams({
    client_id: clientId,
    redirect_uri: redirectUri,
    response_type: 'code',
    access_type: 'offline',
    prompt: 'consent',
    scope: scopes.join(' '),
    state,
  });

  return `${GOOGLE_AUTH_URL}?${params.toString()}`;
}

async function listenForOAuthCode({ authUrl, redirectUri, state }) {
  const redirectUrl = new URL(redirectUri);
  const hostname = redirectUrl.hostname;
  const port = Number(redirectUrl.port || 80);
  const pathname = redirectUrl.pathname;

  return new Promise((resolve, reject) => {
    const server = createServer((request, response) => {
      const requestUrl = new URL(request.url ?? '/', redirectUri);

      if (requestUrl.pathname !== pathname) {
        response.writeHead(404, { 'content-type': 'text/plain; charset=utf-8' });
        response.end('Not found');
        return;
      }

      const error = requestUrl.searchParams.get('error');
      if (error) {
        response.writeHead(200, { 'content-type': 'text/html; charset=utf-8' });
        response.end('<html><body><h2>OAuth failed</h2><p>You can close this tab.</p></body></html>');
        cleanup();
        reject(new Error(`OAuth error: ${error}`));
        return;
      }

      const code = requestUrl.searchParams.get('code');
      const returnedState = requestUrl.searchParams.get('state');

      if (!code) {
        response.writeHead(400, { 'content-type': 'text/plain; charset=utf-8' });
        response.end('Missing code');
        return;
      }

      if (returnedState !== state) {
        response.writeHead(400, { 'content-type': 'text/html; charset=utf-8' });
        response.end('<html><body><h2>State mismatch</h2><p>You can close this tab.</p></body></html>');
        cleanup();
        reject(new Error('OAuth state mismatch'));
        return;
      }

      response.writeHead(200, { 'content-type': 'text/html; charset=utf-8' });
      response.end('<html><body><h2>Authentication successful</h2><p>You can close this tab.</p></body></html>');
      cleanup();
      resolve(code);
    });

    const timer = setTimeout(() => {
      cleanup();
      reject(new Error(`OAuth callback timed out after ${CALLBACK_TIMEOUT_MS / 1000} seconds`));
    }, CALLBACK_TIMEOUT_MS);

    function cleanup() {
      clearTimeout(timer);
      server.close();
    }

    server.once('error', (error) => {
      cleanup();
      reject(new Error(`Failed to bind local callback server on ${hostname}:${port}: ${error.message}`));
    });

    server.listen(port, hostname, () => {
      openBrowser(authUrl);
    });
  });
}

function openBrowser(url) {
  const command = platform() === 'darwin'
    ? 'open'
    : platform() === 'win32'
      ? 'start'
      : 'xdg-open';

  execFile(command, [url], (error) => {
    if (error) {
      process.stderr.write(`Failed to open browser automatically. Open this URL manually:\n${url}\n`);
    }
  });
}

async function exchangeAuthorizationCode({ clientId, clientSecret, redirectUri, code }) {
  const response = await fetch(GOOGLE_TOKEN_URL, {
    method: 'POST',
    headers: { 'content-type': 'application/x-www-form-urlencoded' },
    body: new URLSearchParams({
      code,
      client_id: clientId,
      client_secret: clientSecret,
      redirect_uri: redirectUri,
      grant_type: 'authorization_code',
    }),
  });

  const data = await parseJsonResponse(response, 'OAuth token exchange failed');

  if (typeof data.access_token !== 'string') {
    throw new Error('OAuth token exchange did not return access_token');
  }

  return {
    accessToken: data.access_token,
    refreshToken: data.refresh_token,
    expiresIn: data.expires_in,
    scope: data.scope,
    tokenType: data.token_type,
    clientId,
    clientSecret,
  };
}

async function exchangeRefreshToken({ clientId, clientSecret, refreshToken }) {
  const response = await fetch(GOOGLE_TOKEN_URL, {
    method: 'POST',
    headers: { 'content-type': 'application/x-www-form-urlencoded' },
    body: new URLSearchParams({
      client_id: clientId,
      client_secret: clientSecret,
      refresh_token: refreshToken,
      grant_type: 'refresh_token',
    }),
  });

  const data = await parseJsonResponse(response, 'Refresh token exchange failed');

  if (typeof data.access_token !== 'string') {
    throw new Error('Refresh token exchange did not return access_token');
  }

  return {
    accessToken: data.access_token,
    refreshToken,
    expiresIn: data.expires_in,
    scope: data.scope,
    tokenType: data.token_type,
    clientId,
    clientSecret,
  };
}

async function fetchUserinfo(accessToken) {
  const response = await fetch(GOOGLE_USERINFO_URL, {
    headers: { authorization: `Bearer ${accessToken}` },
  });

  const data = await parseJsonResponse(response, 'Userinfo request failed');

  if (typeof data.email !== 'string') {
    throw new Error('Userinfo response did not contain email');
  }

  return { email: data.email };
}

async function createWatch(accessToken, request) {
  const response = await fetch(`${GMAIL_API_BASE_URL}/${encodeURIComponent(request.userId)}/watch`, {
    method: 'POST',
    headers: {
      authorization: `Bearer ${accessToken}`,
      'content-type': 'application/json',
    },
    body: JSON.stringify(compactObject({
      topicName: request.topicName,
      labelIds: request.labelIds,
      labelFilterBehavior: request.labelFilterBehavior,
    })),
  });

  return parseJsonResponse(response, buildGmailErrorMessage(request.topicName));
}

async function stopWatch(accessToken, userId) {
  const response = await fetch(`${GMAIL_API_BASE_URL}/${encodeURIComponent(userId)}/stop`, {
    method: 'POST',
    headers: {
      authorization: `Bearer ${accessToken}`,
      'content-type': 'application/json',
    },
  });

  await parseJsonResponse(response, 'Gmail stop watch failed');
}

function buildGmailErrorMessage(topicName) {
  return [
    'Gmail watch failed.',
    `Requested topic: ${topicName}`,
    'Common causes:',
    '- Gmail API is not enabled in the same Google Cloud project as the OAuth client.',
    '- The Pub/Sub topic project does not exactly match the OAuth client project.',
    '- The topic does not exist.',
    '- gmail-api-push@system.gserviceaccount.com does not have Pub/Sub publisher access on the topic.',
    '- The authenticated user did not grant a Gmail scope such as gmail.modify.',
  ].join('\n');
}

async function parseJsonResponse(response, contextMessage) {
  const raw = await response.text();
  let data;

  try {
    data = raw.length > 0 ? JSON.parse(raw) : {};
  } catch {
    throw new Error(`${contextMessage}: ${response.status} ${response.statusText}\n${raw}`);
  }

  if (!response.ok) {
    const details = data?.error?.message
      ?? data?.error_description
      ?? JSON.stringify(data);
    throw new Error(`${contextMessage}: ${response.status} ${response.statusText}\n${details}`);
  }

  return data;
}

function compactObject(value) {
  return Object.fromEntries(
    Object.entries(value).filter(([, entryValue]) => {
      if (entryValue === undefined) return false;
      if (Array.isArray(entryValue) && entryValue.length === 0) return false;
      return true;
    }),
  );
}

function parseCsv(value) {
  if (typeof value !== 'string' || value.trim() === '') {
    return [];
  }

  return value
    .split(',')
    .map((entry) => entry.trim())
    .filter(Boolean);
}

function parsePositiveInteger(value, fallback, flagName) {
  if (value === undefined) return fallback;

  const parsed = Number.parseInt(String(value), 10);
  if (!Number.isInteger(parsed) || parsed <= 0) {
    throw new Error(`${flagName} must be a positive integer`);
  }
  return parsed;
}

function parseTopicName(topicName) {
  const match = /^projects\/([^/]+)\/topics\/([^/]+)$/.exec(topicName);
  if (!match) {
    throw new Error(`Invalid topic name: ${topicName}`);
  }

  return {
    projectId: match[1],
    topicId: match[2],
  };
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function watchForIncomingMessagesAndReply({
  accessToken,
  selfEmail,
  userId,
  projectId,
  subscriptionName,
  timeoutSeconds,
  pollIntervalMs,
  baselineHistoryId,
  allowedSenders,
  fixedReplyBody,
}) {
  const startTime = Date.now();
  const deadline = startTime + timeoutSeconds * 1000;
  const seenMessageIds = new Set();
  let lastHistoryId = baselineHistoryId;
  let repliedCount = 0;

  while (Date.now() < deadline) {
    const messages = await pullSubscriptionMessages({ projectId, subscriptionName });

    if (messages.length === 0) {
      await sleep(pollIntervalMs);
      continue;
    }

    for (const envelope of messages) {
      try {
        const notification = decodePubsubNotification(envelope);

        if (notification.emailAddress !== selfEmail) {
          continue;
        }

        if (compareHistoryIds(notification.historyId, lastHistoryId) <= 0) {
          continue;
        }

        const history = await listHistory(accessToken, {
          userId,
          startHistoryId: lastHistoryId,
        });

        lastHistoryId = String(history.historyId ?? notification.historyId);

        const candidateIds = extractCandidateMessageIds(history);
        for (const messageId of candidateIds) {
          if (seenMessageIds.has(messageId)) {
            continue;
          }
          seenMessageIds.add(messageId);

          try {
            const message = await getMessageMetadata(accessToken, { userId, messageId });
            const sender = extractSenderInfo(message);

            if (!sender.email || sender.email.toLowerCase() === selfEmail.toLowerCase()) {
              continue;
            }

            if (!matchesAllowedSender(sender, allowedSenders)) {
              continue;
            }

            const body = fixedReplyBody ?? generateReplyBody({
              sender,
              message,
            });

            const reply = await sendReply(accessToken, {
              userId,
              threadId: String(message.threadId ?? message.id ?? ''),
              replyTo: sender.email,
              subject: sender.subject,
              messageIdHeader: sender.messageIdHeader,
              referencesHeader: sender.referencesHeader,
              body,
            });

            repliedCount += 1;
            process.stdout.write(
              [
                `Replied to ${formatSenderLabel(sender)}`,
                `Message ID: ${messageId}`,
                `Reply message ID: ${String(reply.id ?? '')}`,
              ].join('\n') + '\n',
            );
          } catch (error) {
            const message = error instanceof Error ? error.message : String(error);
            process.stderr.write(`Skipping message ${messageId}: ${message}\n`);
          }
        }
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        process.stderr.write(`Skipping notification: ${message}\n`);
      }
    }

    await sleep(pollIntervalMs);
  }

  return {
    repliedCount,
    processedCount: seenMessageIds.size,
    elapsedSeconds: Math.ceil((Date.now() - startTime) / 1000),
  };
}

async function saveTokens(tokensPath, credentials, tokens) {
  if (typeof tokens.refreshToken !== 'string' || tokens.refreshToken.length === 0) {
    throw new Error('No refresh token returned by Google; cannot save reusable tokens');
  }

  const content = {
    type: 'authorized_user',
    client_id: credentials.clientId,
    client_secret: credentials.clientSecret,
    refresh_token: tokens.refreshToken,
    scope: tokens.scope,
  };

  await writeFile(tokensPath, `${JSON.stringify(content, null, 2)}\n`, 'utf8');
}

async function ensurePubsubResources({ projectId, topicId, subscriptionName }) {
  const topicExists = await gcloudExists([
    'pubsub', 'topics', 'describe', topicId,
    '--project', projectId,
    '--format=json',
  ]);

  if (!topicExists) {
    process.stdout.write(`Creating topic ${topicId} in project ${projectId}.\n`);
    await runCommand('gcloud', [
      'pubsub', 'topics', 'create', topicId,
      '--project', projectId,
    ]);
  }

  process.stdout.write('Granting Gmail push publisher permission on the topic.\n');
  await runCommand('gcloud', [
    'pubsub', 'topics', 'add-iam-policy-binding', topicId,
    '--project', projectId,
    '--member', 'serviceAccount:gmail-api-push@system.gserviceaccount.com',
    '--role', 'roles/pubsub.publisher',
    '--quiet',
  ]);

  const subscriptionExists = await gcloudExists([
    'pubsub', 'subscriptions', 'describe', subscriptionName,
    '--project', projectId,
    '--format=json',
  ]);

  if (!subscriptionExists) {
    process.stdout.write(`Creating subscription ${subscriptionName}.\n`);
    await runCommand('gcloud', [
      'pubsub', 'subscriptions', 'create', subscriptionName,
      '--project', projectId,
      '--topic', topicId,
      '--ack-deadline', '60',
    ]);
  }
}

async function gcloudExists(args) {
  try {
    await runCommand('gcloud', args);
    return true;
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    if (
      message.includes('NOT_FOUND')
      || message.includes('Resource not found')
      || message.includes('does not exist')
    ) {
      return false;
    }
    throw error;
  }
}

async function waitForIncomingMessageAndReply({
  accessToken,
  selfEmail,
  userId,
  projectId,
  subscriptionName,
  replyBody,
  timeoutSeconds,
  pollIntervalMs,
  baselineHistoryId,
}) {
  let lastHistoryId = baselineHistoryId;
  const deadline = Date.now() + timeoutSeconds * 1000;

  while (Date.now() < deadline) {
    const messages = await pullSubscriptionMessages({ projectId, subscriptionName });

    if (messages.length === 0) {
      await sleep(pollIntervalMs);
      continue;
    }

    for (const envelope of messages) {
      const notification = decodePubsubNotification(envelope);

      if (notification.emailAddress !== selfEmail) {
        continue;
      }

      if (compareHistoryIds(notification.historyId, lastHistoryId) <= 0) {
        continue;
      }

      const history = await listHistory(accessToken, {
        userId,
        startHistoryId: lastHistoryId,
      });

      lastHistoryId = String(history.historyId ?? notification.historyId);

      const candidates = extractCandidateMessageIds(history);
      for (const messageId of candidates) {
        const message = await getMessageMetadata(accessToken, { userId, messageId });
        const candidate = toReplyCandidate(message, selfEmail);
        if (!candidate) {
          continue;
        }

        const reply = await sendReply(accessToken, {
          userId,
          threadId: String(message.threadId),
          replyTo: candidate.replyTo,
          subject: candidate.subject,
          messageIdHeader: candidate.messageIdHeader,
          referencesHeader: candidate.referencesHeader,
          body: replyBody,
        });

        return {
          originalMessageId: messageId,
          threadId: String(message.threadId),
          replyTo: candidate.replyTo,
          replyMessageId: String(reply.id ?? ''),
        };
      }
    }

    await sleep(pollIntervalMs);
  }

  throw new Error(`Timed out after ${timeoutSeconds} seconds waiting for a new incoming message`);
}

async function pullSubscriptionMessages({ projectId, subscriptionName }) {
  const stdout = await runCommand('gcloud', [
    'pubsub', 'subscriptions', 'pull', subscriptionName,
    '--project', projectId,
    '--limit', '10',
    '--auto-ack',
    '--format=json',
  ]);

  const trimmed = stdout.trim();
  if (trimmed === '') {
    return [];
  }

  const parsed = JSON.parse(trimmed);
  return Array.isArray(parsed) ? parsed : [];
}

function decodePubsubNotification(envelope) {
  const encoded = envelope?.message?.data;
  if (typeof encoded !== 'string' || encoded.length === 0) {
    throw new Error(`Pub/Sub message did not contain message.data: ${JSON.stringify(envelope)}`);
  }

  const normalized = encoded.replace(/-/g, '+').replace(/_/g, '/');
  const padded = normalized.padEnd(Math.ceil(normalized.length / 4) * 4, '=');
  return JSON.parse(Buffer.from(padded, 'base64').toString('utf8'));
}

function compareHistoryIds(left, right) {
  const leftValue = BigInt(String(left));
  const rightValue = BigInt(String(right));
  return leftValue === rightValue ? 0 : leftValue > rightValue ? 1 : -1;
}

function extractSenderInfo(message) {
  const headers = Array.isArray(message?.payload?.headers) ? message.payload.headers : [];
  const getHeader = (name) => {
    const match = headers.find((header) => String(header?.name ?? '').toLowerCase() === name.toLowerCase());
    return typeof match?.value === 'string' ? match.value : '';
  };

  const fromHeader = getHeader('From');
  const replyToHeader = getHeader('Reply-To');
  const sourceHeader = replyToHeader || fromHeader;
  const email = extractEmailAddress(sourceHeader);
  const displayName = extractDisplayName(sourceHeader) || extractDisplayName(fromHeader);

  return {
    email,
    displayName,
    rawFrom: fromHeader,
    rawReplyTo: replyToHeader,
    subject: getHeader('Subject') || '(no subject)',
    messageIdHeader: getHeader('Message-ID'),
    referencesHeader: getHeader('References'),
    snippet: typeof message?.snippet === 'string' ? message.snippet : '',
  };
}

function extractDisplayName(value) {
  if (typeof value !== 'string') {
    return '';
  }

  const trimmed = value.trim();
  const angleMatch = /^(.*)<[^>]+>$/.exec(trimmed);
  if (angleMatch) {
    return angleMatch[1].replace(/^"|"$/g, '').trim();
  }

  if (trimmed.includes('@')) {
    return '';
  }

  return trimmed.replace(/^"|"$/g, '').trim();
}

function matchesAllowedSender(sender, allowedSenders) {
  if (allowedSenders.length === 0) {
    return true;
  }

  const haystack = [
    sender.email,
    sender.displayName,
    sender.rawFrom,
    sender.rawReplyTo,
  ]
    .filter(Boolean)
    .map(value => value.toLowerCase());

  return allowedSenders.some((filter) => {
    const normalizedFilter = filter.toLowerCase();
    return haystack.some(value => value.includes(normalizedFilter));
  });
}

function formatSenderLabel(sender) {
  if (sender.displayName && sender.email) {
    return `${sender.displayName} <${sender.email}>`;
  }
  if (sender.email) {
    return sender.email;
  }
  return sender.displayName || 'unknown sender';
}

function generateReplyBody({ sender, message }) {
  const greetingName = deriveGreetingName(sender);
  const greeting = greetingName
    ? `Ciao ${greetingName},`
    : 'Ciao,';
  const context = [
    sender.subject,
    typeof message?.snippet === 'string' ? message.snippet : '',
  ]
    .filter(Boolean)
    .join(' ')
    .toLowerCase();

  return [
    greeting,
    '',
    chooseReplyLine(context),
    '',
    chooseReplyClosing(context),
  ].join('\n');
}

function chooseReplyLine(context) {
  if (containsAny(context, ['grazie', 'thanks'])) {
    return 'Figurati, tutto a posto.';
  }

  if (containsAny(context, ['come stai', 'come va', 'come te'])) {
    return 'Tutto bene, grazie.';
  }

  if (containsAny(context, ['puoi', 'mi puoi', 'ci puoi', 'fammi sapere', 'ti va'])) {
    return 'Sì, certo.';
  }

  if (containsAny(context, ['ok', 'va bene', 'perfetto'])) {
    return 'Perfetto, grazie.';
  }

  if (containsAny(context, ['ozempic', 'salute', 'medico'])) {
    return "Va bene, grazie per l'aggiornamento.";
  }

  return 'Ricevuto, grazie.';
}

function chooseReplyClosing(context) {
  if (containsAny(context, ['grazie', 'thanks', 'ok', 'va bene'])) {
    return 'A dopo';
  }

  return 'Ci sentiamo';
}

function containsAny(text, needles) {
  return needles.some(needle => text.includes(needle));
}

function deriveGreetingName(sender) {
  const displayName = typeof sender.displayName === 'string' ? sender.displayName.trim() : '';
  if (displayName) {
    const firstToken = displayName.split(/\s+/)[0];
    const cleaned = firstToken.replace(/\d+$/g, '').replace(/[^A-Za-zÀ-ÿ'-]/g, '').trim();
    if (cleaned) {
      return cleaned;
    }
  }

  const emailLocalPart = typeof sender.email === 'string'
    ? sender.email.split('@')[0]
    : '';

  const firstChunk = emailLocalPart.split(/[._-]/)[0] ?? '';
  const cleanedChunk = firstChunk.replace(/\d+$/g, '').replace(/[^A-Za-zÀ-ÿ'-]/g, '').trim();
  if (cleanedChunk) {
    return cleanedChunk.charAt(0).toUpperCase() + cleanedChunk.slice(1);
  }

  return '';
}

async function listHistory(accessToken, { userId, startHistoryId }) {
  const url = new URL(`${GMAIL_API_BASE_URL}/${encodeURIComponent(userId)}/history`);
  url.searchParams.set('startHistoryId', String(startHistoryId));
  url.searchParams.set('historyTypes', 'messageAdded');
  url.searchParams.set('maxResults', '20');

  const response = await fetch(url, {
    headers: {
      authorization: `Bearer ${accessToken}`,
    },
  });

  return parseJsonResponse(response, 'Gmail history.list failed');
}

function extractCandidateMessageIds(historyResponse) {
  const historyEntries = Array.isArray(historyResponse.history) ? historyResponse.history : [];
  const ids = new Set();

  for (const entry of historyEntries) {
    const messagesAdded = Array.isArray(entry.messagesAdded) ? entry.messagesAdded : [];
    for (const added of messagesAdded) {
      const messageId = added?.message?.id;
      if (typeof messageId === 'string' && messageId.length > 0) {
        ids.add(messageId);
      }
    }
  }

  return [...ids];
}

async function getMessageMetadata(accessToken, { userId, messageId }) {
  const url = new URL(`${GMAIL_API_BASE_URL}/${encodeURIComponent(userId)}/messages/${encodeURIComponent(messageId)}`);
  url.searchParams.set('format', 'metadata');
  for (const header of ['From', 'Reply-To', 'Subject', 'Message-ID', 'References', 'Auto-Submitted', 'Precedence']) {
    url.searchParams.append('metadataHeaders', header);
  }

  const response = await fetch(url, {
    headers: {
      authorization: `Bearer ${accessToken}`,
    },
  });

  return parseJsonResponse(response, 'Gmail messages.get failed');
}

function toReplyCandidate(message, selfEmail) {
  const headers = Array.isArray(message?.payload?.headers) ? message.payload.headers : [];
  const getHeader = (name) => {
    const match = headers.find((header) => String(header?.name ?? '').toLowerCase() === name.toLowerCase());
    return typeof match?.value === 'string' ? match.value : '';
  };

  const from = getHeader('From');
  const replyTo = getHeader('Reply-To') || from;
  const senderEmail = extractEmailAddress(replyTo || from);
  const autoSubmitted = getHeader('Auto-Submitted').toLowerCase();
  const precedence = getHeader('Precedence').toLowerCase();

  if (!senderEmail || senderEmail.toLowerCase() === selfEmail.toLowerCase()) {
    return null;
  }

  if (autoSubmitted && autoSubmitted !== 'no') {
    return null;
  }

  if (['bulk', 'list', 'junk'].includes(precedence)) {
    return null;
  }

  return {
    replyTo: senderEmail,
    subject: getHeader('Subject') || '(no subject)',
    messageIdHeader: getHeader('Message-ID'),
    referencesHeader: getHeader('References'),
  };
}

function extractEmailAddress(value) {
  const match = /([A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,})/i.exec(value);
  return match ? match[1] : '';
}

function toBase64Url(value) {
  return Buffer.from(value, 'utf8')
    .toString('base64')
    .replace(/\+/g, '-')
    .replace(/\//g, '_')
    .replace(/=+$/g, '');
}

async function sendReply(accessToken, {
  userId,
  threadId,
  replyTo,
  subject,
  messageIdHeader,
  referencesHeader,
  body,
}) {
  const lines = [
    `To: ${replyTo}`,
    `Subject: ${normalizeReplySubject(subject)}`,
    'Content-Type: text/plain; charset="UTF-8"',
    'MIME-Version: 1.0',
  ];

  if (messageIdHeader) {
    lines.push(`In-Reply-To: ${messageIdHeader}`);
    const referencesValue = referencesHeader ? `${referencesHeader} ${messageIdHeader}` : messageIdHeader;
    lines.push(`References: ${referencesValue}`);
  }

  lines.push('', body);

  const response = await fetch(`${GMAIL_API_BASE_URL}/${encodeURIComponent(userId)}/messages/send`, {
    method: 'POST',
    headers: {
      authorization: `Bearer ${accessToken}`,
      'content-type': 'application/json',
    },
    body: JSON.stringify({
      threadId,
      raw: toBase64Url(lines.join('\r\n')),
    }),
  });

  return parseJsonResponse(response, 'Gmail reply send failed');
}

function normalizeReplySubject(subject) {
  return /^re:/i.test(subject) ? subject : `Re: ${subject}`;
}

async function runCommand(command, args) {
  return new Promise((resolve, reject) => {
    execFile(command, args, { maxBuffer: 10 * 1024 * 1024 }, (error, stdout, stderr) => {
      if (error) {
        const details = stderr.trim() || stdout.trim() || error.message;
        reject(new Error(`${command} ${args.join(' ')} failed: ${details}`));
        return;
      }
      resolve(stdout);
    });
  });
}

function printHelp() {
  process.stdout.write(
    [
      'Usage:',
      '  node scripts/test-gmail-watch.mjs --credentials <oauth-client.json> --topic <projects/.../topics/...> [options]',
      '',
      'Required for watch mode:',
      '  --credentials          Path to Google OAuth client JSON (web or installed)',
      '  --topic                Fully qualified Pub/Sub topic name',
      '',
      'Options:',
      '  --mode watch|stop|reply-once|reply-window  Default: watch',
      '  --labels INBOX,UNREAD  Optional comma-separated Gmail label IDs',
      '  --label-filter-behavior INCLUDE|EXCLUDE',
      '  --email me             Gmail user ID, default: me',
      '  --subscription <name>  Pub/Sub subscription name for reply mode',
      '  --allowed-senders <list>  Comma-separated sender filters for reply-window mode',
      '  --reply-body <text>    Fixed reply body; omit to generate locally',
      '  --timeout-seconds <n>  Wait timeout for reply-once/reply-window mode, default: 600',
      '  --poll-interval-ms <n> Pull interval for reply-once/reply-window mode, default: 4000',
      '  --print-auth-url       Print an OAuth consent URL for manual login and exit',
      '  --callback-url <url>   Full localhost callback URL copied from the browser after consent',
      '  --redirect-uri <uri>   Force one registered redirect URI from the client JSON',
      '  --save-tokens <path>   Persist refresh token for later runs',
      '  --tokens <path>        Reuse a saved authorized_user token file instead of interactive login',
      '  --help                 Show this help',
      '',
      'Examples:',
      '  node scripts/test-gmail-watch.mjs \\',
      '    --credentials "/path/client_secret.json" \\',
      '    --topic "projects/giulio-leone/topics/gmail-push" \\',
      '    --labels INBOX \\',
      '    --save-tokens ./.tmp/gmail-watch-token.json',
      '',
      '  node scripts/test-gmail-watch.mjs \\',
      '    --tokens ./.tmp/gmail-watch-token.json \\',
      '    --topic "projects/giulio-leone/topics/gmail-push"',
      '',
      '  node scripts/test-gmail-watch.mjs \\',
      '    --tokens ./.tmp/gmail-watch-token.json \\',
      '    --mode stop',
      '',
      '  node scripts/test-gmail-watch.mjs \\',
      '    --credentials "/path/client_secret.json" \\',
      '    --topic "projects/giulio-leone/topics/gmail-push" \\',
      '    --mode reply-window \\',
      '    --allowed-senders "giulio97.leone@gmail.com,antonella sacchini"',
      '',
      '  node scripts/test-gmail-watch.mjs \\',
      '    --credentials "/path/client_secret.json" \\',
      '    --email "giulioleone097@gmail.com" \\',
      '    --print-auth-url',
    ].join('\n') + '\n',
  );
}

main().catch((error) => {
  const message = error instanceof Error ? error.message : String(error);
  process.stderr.write(`${message}\n`);
  process.exitCode = 1;
});
