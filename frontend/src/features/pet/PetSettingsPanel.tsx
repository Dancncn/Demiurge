import { convertFileSrc } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";
import * as api from "@/lib/api";
import type { InstalledPet } from "@/lib/types";
import { DownloadIcon, TrashIcon } from "@/shared/components/Icons";
import { useI18n } from "@/lib/i18n";

interface Props {
  currentPet: string;
  onSelect: (id: string) => void;
}

const IS_TAURI = "__TAURI_INTERNALS__" in window;

export function PetSettingsPanel({ currentPet, onSelect }: Props) {
  const { t } = useI18n();
  const inputRef = useRef<HTMLInputElement | null>(null);
  const [pets, setPets] = useState<InstalledPet[]>([]);
  const [status, setStatus] = useState("");
  const [isBusy, setIsBusy] = useState(false);

  const refresh = () => api.petList().then(setPets, (error) => setStatus(String(error)));

  useEffect(() => {
    void refresh();
    let unlisten: (() => void) | undefined;
    void api.listenPetCatalogUpdated(setPets).then((dispose) => {
      unlisten = dispose;
    });
    return () => unlisten?.();
  }, []);

  async function handleImport(file?: File | null) {
    if (!file) return;
    setIsBusy(true);
    setStatus("");
    try {
      const bytes = Array.from(new Uint8Array(await file.arrayBuffer()));
      const installed = await api.petImport(file.name, bytes);
      await refresh();
      onSelect(installed.id);
      setStatus(t("settings.pet.imported", { name: installed.name }));
    } catch (error) {
      setStatus(String(error));
    } finally {
      setIsBusy(false);
      if (inputRef.current) inputRef.current.value = "";
    }
  }

  async function handleRemove(id: string) {
    setIsBusy(true);
    setStatus("");
    try {
      await api.petRemove(id);
      if (currentPet === id) onSelect("");
      await refresh();
    } catch (error) {
      setStatus(String(error));
    } finally {
      setIsBusy(false);
    }
  }

  return (
    <section className="pet-settings-panel">
      <div className="mb-3 flex items-center justify-between gap-3">
        <div>
          <div className="text-[14px] font-semibold text-[#202124]">{t("settings.pet.title")}</div>
          <div className="mt-1 text-[12px] text-[#707781]">{t("settings.pet.description")}</div>
        </div>
        <label className="md-button md-button-outlined cf-press inline-flex cursor-pointer items-center gap-1.5 px-3">
          <DownloadIcon size={15} />
          <span>{isBusy ? t("settings.pet.importing") : t("settings.pet.import")}</span>
          <input
            ref={inputRef}
            className="hidden"
            type="file"
            accept=".demipet,application/zip"
            disabled={isBusy}
            onChange={(event) => void handleImport(event.target.files?.[0])}
          />
        </label>
      </div>

      <div className="grid grid-cols-2 gap-2 lg:grid-cols-3">
        {pets.map((pet) => {
          const isSelected = currentPet === pet.id;
          return (
            <div key={pet.id} className={`pet-picker-item ${isSelected ? "is-selected" : ""}`}>
              <button type="button" className="min-w-0 flex-1 text-left" onClick={() => onSelect(pet.id)}>
                <div className="flex items-center gap-2">
                  {pet.thumbnailPath ? (
                    <img
                      src={IS_TAURI ? convertFileSrc(pet.thumbnailPath) : pet.thumbnailPath}
                      alt=""
                      className="size-10 shrink-0 object-contain"
                    />
                  ) : (
                    <div className="size-10 shrink-0 bg-[#eef1f5]" />
                  )}
                  <div className="min-w-0">
                    <div className="truncate text-[13px] font-medium text-[#202124]">{pet.name}</div>
                    <div className="truncate text-[11px] text-[#737b86]">v{pet.version}</div>
                  </div>
                </div>
              </button>
              <button
                type="button"
                className="md-icon-button grid size-7 shrink-0 place-items-center"
                title={t("settings.pet.remove")}
                aria-label={t("settings.pet.remove")}
                disabled={isBusy}
                onClick={() => void handleRemove(pet.id)}
              >
                <TrashIcon size={14} />
              </button>
            </div>
          );
        })}
      </div>
      {!pets.length && <div className="py-4 text-[12px] text-[#737b86]">{t("settings.pet.empty")}</div>}
      {status && <div className="mt-2 text-[12px] text-[#59616d]">{status}</div>}
    </section>
  );
}
