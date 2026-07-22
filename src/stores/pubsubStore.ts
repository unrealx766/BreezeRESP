import { defineStore } from "pinia";
import { ref } from "vue";
import { tauriApi, type PubSubEvent } from "@/services/tauriApi";
import type { UnlistenFn } from "@tauri-apps/api/event";

/** Maximum number of received messages retained per connection. */
const MAX_MESSAGES = 500;

/** Maximum number of published-message history entries retained per connection. */
const MAX_PUBLISHES = 200;

export interface PubSubMessageItem {
  channel: string;
  message: string;
  timestamp: number;
}

/** A record of one message published by the user, with its delivery count. */
export interface PubSubPublishItem {
  channel: string;
  message: string;
  /** Number of subscribers that received the message (PUBLISH return value). */
  count: number;
  timestamp: number;
}

/**
 * Holds Pub/Sub subscriptions and received messages, keyed by connection id.
 *
 * State lives in the store (not the page component) so it survives tab
 * switches — the PubSub page unmounts on navigation. A single app-wide
 * `pubsub-message` listener keeps filling the message buffer even while the
 * page is not mounted.
 */
export const usePubsubStore = defineStore("pubsub", () => {
  /** connectionId → active (sorted) channel names */
  const subscriptionsMap = ref<Record<string, string[]>>({});
  /** connectionId → received messages (newest first) */
  const messagesMap = ref<Record<string, PubSubMessageItem[]>>({});
  /** connectionId → published-message history (newest first) */
  const publishesMap = ref<Record<string, PubSubPublishItem[]>>({});

  /** Handle to the global event listener; set once by `init()`. */
  let unlisten: UnlistenFn | null = null;

  function _pushMessage(msg: PubSubEvent): void {
    const list = messagesMap.value[msg.connectionId]
      ? [...messagesMap.value[msg.connectionId]]
      : [];
    list.unshift({ channel: msg.channel, message: msg.message, timestamp: msg.timestamp });
    if (list.length > MAX_MESSAGES) list.length = MAX_MESSAGES;
    messagesMap.value = { ...messagesMap.value, [msg.connectionId]: list };
  }

  /** Register the app-wide real-time message listener (idempotent). */
  async function init(): Promise<void> {
    if (unlisten) return;
    unlisten = await tauriApi.pubsub.onMessage((msg) => _pushMessage(msg));
  }

  /** Read the active channel list for a connection. */
  function channelsOf(connectionId: string | null): string[] {
    return connectionId ? subscriptionsMap.value[connectionId] ?? [] : [];
  }

  /** Read the received messages for a connection. */
  function messagesOf(connectionId: string | null): PubSubMessageItem[] {
    return connectionId ? messagesMap.value[connectionId] ?? [] : [];
  }

  /** Read the published-message history for a connection. */
  function publishesOf(connectionId: string | null): PubSubPublishItem[] {
    return connectionId ? publishesMap.value[connectionId] ?? [] : [];
  }

  /** Record a message the user just published (newest first). */
  function addPublish(connectionId: string, item: PubSubPublishItem): void {
    const list = publishesMap.value[connectionId] ? [...publishesMap.value[connectionId]] : [];
    list.unshift(item);
    if (list.length > MAX_PUBLISHES) list.length = MAX_PUBLISHES;
    publishesMap.value = { ...publishesMap.value, [connectionId]: list };
  }

  /** Clear the published-message history for a connection. */
  function clearPublishes(connectionId: string): void {
    publishesMap.value = { ...publishesMap.value, [connectionId]: [] };
  }

  async function subscribe(connectionId: string, channel: string): Promise<string[]> {
    const result = await tauriApi.pubsub.subscribe(connectionId, channel);
    subscriptionsMap.value = { ...subscriptionsMap.value, [connectionId]: result };
    return result;
  }

  async function unsubscribe(connectionId: string, channel: string): Promise<string[]> {
    const result = await tauriApi.pubsub.unsubscribe(connectionId, channel);
    subscriptionsMap.value = { ...subscriptionsMap.value, [connectionId]: result };
    // Drop buffered messages for the channel we just left.
    const remaining = (messagesMap.value[connectionId] ?? []).filter((m) => m.channel !== channel);
    messagesMap.value = { ...messagesMap.value, [connectionId]: remaining };
    return result;
  }

  async function unsubscribeAll(connectionId: string): Promise<void> {
    await tauriApi.pubsub.unsubscribe(connectionId);
    subscriptionsMap.value = { ...subscriptionsMap.value, [connectionId]: [] };
    messagesMap.value = { ...messagesMap.value, [connectionId]: [] };
  }

  /** Clear only the received-message buffer for a connection. */
  function clearMessages(connectionId: string): void {
    messagesMap.value = { ...messagesMap.value, [connectionId]: [] };
  }

  /**
   * Drop all local Pub/Sub state for a connection. Called when the connection
   * disconnects / switches DB / is removed / is lost — the backend already
   * tears down its dedicated listener in those cases.
   */
  function clearConnection(connectionId: string): void {
    if (connectionId in subscriptionsMap.value) {
      const next = { ...subscriptionsMap.value };
      delete next[connectionId];
      subscriptionsMap.value = next;
    }
    if (connectionId in messagesMap.value) {
      const next = { ...messagesMap.value };
      delete next[connectionId];
      messagesMap.value = next;
    }
    if (connectionId in publishesMap.value) {
      const next = { ...publishesMap.value };
      delete next[connectionId];
      publishesMap.value = next;
    }
  }

  return {
    subscriptionsMap,
    messagesMap,
    publishesMap,
    init,
    channelsOf,
    messagesOf,
    publishesOf,
    addPublish,
    clearPublishes,
    subscribe,
    unsubscribe,
    unsubscribeAll,
    clearMessages,
    clearConnection,
  };
});
