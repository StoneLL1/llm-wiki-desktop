import { create } from "zustand";

import type { ImportedSource, ImportPreview } from "../types/import";

interface ImportState {
  preview: ImportPreview | null;
  importedSources: ImportedSource[];
  isConfirming: boolean;
  selectedSourcePath: string | null;
  urlDialogOpen: boolean;
  folderDialogOpen: boolean;
  createCheckpoint: boolean;
  compileAfterImport: boolean;
  setPreview: (preview: ImportPreview | null) => void;
  setImportedSources: (sources: ImportedSource[]) => void;
  setIsConfirming: (confirming: boolean) => void;
  setSelectedSourcePath: (path: string | null) => void;
  setUrlDialogOpen: (open: boolean) => void;
  setFolderDialogOpen: (open: boolean) => void;
  setCreateCheckpoint: (create: boolean) => void;
  setCompileAfterImport: (compile: boolean) => void;
  reset: () => void;
}

export const useImportStore = create<ImportState>((set) => ({
  preview: null,
  importedSources: [],
  isConfirming: false,
  selectedSourcePath: null,
  urlDialogOpen: false,
  folderDialogOpen: false,
  createCheckpoint: true,
  compileAfterImport: true,
  setPreview: (preview) =>
    set((current) => ({
      preview,
      selectedSourcePath:
        preview && current.selectedSourcePath
          ? (preview.files.find((f) => f.sourcePath === current.selectedSourcePath)
              ?.sourcePath ?? preview.files[0]?.sourcePath ?? null)
          : (preview?.files[0]?.sourcePath ?? null),
    })),
  setImportedSources: (importedSources) => set({ importedSources }),
  setIsConfirming: (isConfirming) => set({ isConfirming }),
  setSelectedSourcePath: (selectedSourcePath) => set({ selectedSourcePath }),
  setUrlDialogOpen: (urlDialogOpen) => set({ urlDialogOpen }),
  setFolderDialogOpen: (folderDialogOpen) => set({ folderDialogOpen }),
  setCreateCheckpoint: (createCheckpoint) => set({ createCheckpoint }),
  setCompileAfterImport: (compileAfterImport) => set({ compileAfterImport }),
  reset: () =>
    set({
      preview: null,
      selectedSourcePath: null,
      isConfirming: false,
      urlDialogOpen: false,
      folderDialogOpen: false,
    }),
}));
