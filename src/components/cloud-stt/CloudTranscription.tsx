import React, { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import i18n from "i18next";
import { invoke } from "@tauri-apps/api/core";
import { Alert } from "@/components/ui/Alert";
import { Button } from "@/components/ui/Button";
import { Input } from "@/components/ui/Input";
import { Select } from "@/components/ui/Select";
import { Switch } from "@/components/ui/Switch";

// Fork-only strings live here, in their own namespace, so upstream's locale
// files stay untouched. Other languages fall back to English.
const NAMESPACE = "cloudStt";
const EN_STRINGS = {
  title: "Cloud transcription (OpenRouter)",
  description:
    "Send recordings to an OpenRouter speech-to-text model instead of transcribing on this computer.",
  privacy:
    "While this is on, your audio is uploaded to OpenRouter and the model's provider.",
  apiKey: "OpenRouter API key",
  apiKeySaved: "A key is saved. Paste a new one to replace it.",
  apiKeyPlaceholder: "sk-or-...",
  loadModels: "Load models",
  loading: "Loading…",
  model: "Model",
  modelPlaceholder: "Load models, then pick one",
  use: "Use cloud transcription",
  needsSetup: "Add a key and pick a model to turn this on.",
};

// Called at render, not import: this module loads before i18n is initialized.
const ensureStrings = () => {
  if (!i18n.hasResourceBundle("en", NAMESPACE)) {
    i18n.addResourceBundle("en", NAMESPACE, EN_STRINGS);
  }
};

interface Status {
  enabled: boolean;
  model: string;
  hasKey: boolean;
}

interface CloudModel {
  id: string;
  name: string;
}

export const CloudTranscription: React.FC = () => {
  ensureStrings();
  const { t } = useTranslation(NAMESPACE);
  const [status, setStatus] = useState<Status | null>(null);
  const [key, setKey] = useState("");
  const [models, setModels] = useState<CloudModel[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setStatus(await invoke<Status>("cloud_stt_get_status"));
  }, []);

  useEffect(() => {
    refresh().catch((e) => setError(String(e)));
  }, [refresh]);

  const run = async (work: () => Promise<void>) => {
    setError(null);
    try {
      await work();
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  };

  const loadModels = async () => {
    setBusy(true);
    await run(async () => {
      if (key.trim()) {
        await invoke("cloud_stt_set_api_key", { apiKey: key });
        setKey("");
      }
      setModels(await invoke<CloudModel[]>("cloud_stt_list_models"));
    });
    setBusy(false);
  };

  if (!status) return null;

  const options = models.map((m) => ({ value: m.id, label: m.name }));
  // Show the saved pick even before the list has been loaded this session.
  if (status.model && !options.some((o) => o.value === status.model)) {
    options.unshift({ value: status.model, label: status.model });
  }
  const ready = status.hasKey && status.model !== "";

  return (
    <section className="flex flex-col gap-[10px] rounded-lg border border-line p-[12px]">
      <div className="flex items-start justify-between gap-[12px]">
        <div>
          <h2 className="text-base font-semibold">{t("title")}</h2>
          <p className="text-sm text-ink-2">{t("description")}</p>
        </div>
        <Switch
          checked={status.enabled && ready}
          disabled={!ready}
          ariaLabel={t("use")}
          onChange={(enabled) =>
            run(() => invoke("cloud_stt_set_enabled", { enabled }))
          }
        />
      </div>
      {!ready && <p className="text-sm text-ink-3">{t("needsSetup")}</p>}

      <label className="flex flex-col gap-[4px] text-sm font-medium">
        {t("apiKey")}
        <div className="flex gap-[8px]">
          <Input
            type="password"
            autoComplete="off"
            variant="compact"
            className="flex-1"
            value={key}
            placeholder={
              status.hasKey ? t("apiKeySaved") : t("apiKeyPlaceholder")
            }
            onChange={(e) => setKey(e.target.value)}
          />
          <Button
            variant="secondary"
            size="sm"
            disabled={busy || (!key.trim() && !status.hasKey)}
            onClick={loadModels}
          >
            {busy ? t("loading") : t("loadModels")}
          </Button>
        </div>
      </label>

      <div className="flex flex-col gap-[4px] text-sm font-medium">
        {t("model")}
        <Select
          value={status.model || null}
          options={options}
          placeholder={t("modelPlaceholder")}
          disabled={!status.hasKey}
          onChange={(model) =>
            model && run(() => invoke("cloud_stt_set_model", { model }))
          }
        />
      </div>

      {status.enabled && ready && (
        <Alert variant="warning">{t("privacy")}</Alert>
      )}
      {error && <Alert variant="error">{error}</Alert>}
    </section>
  );
};
