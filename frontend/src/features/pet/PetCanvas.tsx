import { convertFileSrc } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";
import * as api from "@/lib/api";
import type { PetAction, PetBinding, ResolvedPet } from "@/lib/types";
import { semanticBindingKey, type PetSemanticAction } from "@/features/pet/petRuntime";

interface Props {
  petId: string;
  action: PetSemanticAction;
  revision?: number;
  onAnimationComplete: () => void;
  onTap: () => void;
  onHoverChange: (hovered: boolean) => void;
  onError?: (message: string) => void;
}

const IS_TAURI = "__TAURI_INTERNALS__" in window;

function localAssetUrl(path: string) {
  return IS_TAURI ? convertFileSrc(path) : path;
}

function bindingFor(pet: ResolvedPet, action: PetSemanticAction): PetBinding {
  const idle = pet.manifest.bindings["system.idle"];
  return pet.manifest.bindings[semanticBindingKey[action]] ?? idle;
}

export function PetCanvas({
  petId,
  action,
  revision = 0,
  onAnimationComplete,
  onTap,
  onHoverChange,
  onError,
}: Props) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const completionRef = useRef(onAnimationComplete);
  const [pet, setPet] = useState<ResolvedPet | null>(null);

  useEffect(() => {
    completionRef.current = onAnimationComplete;
  }, [onAnimationComplete]);

  useEffect(() => {
    let cancelled = false;
    setPet(null);
    if (!petId) return;
    void api.petResolve(petId).then(
      (resolved) => {
        if (!cancelled) setPet(resolved);
      },
      (error) => {
        if (!cancelled) onError?.(String(error));
      },
    );
    return () => {
      cancelled = true;
    };
  }, [onError, petId, revision]);

  useEffect(() => {
    if (!pet || !canvasRef.current) return;
    const canvas = canvasRef.current;
    const context = canvas.getContext("2d");
    if (!context) return;
    const renderer = pet.manifest.renderer;
    const binding = bindingFor(pet, action);
    const clip: PetAction | undefined = pet.manifest.actions[binding.action];
    if (!clip) return;

    canvas.width = renderer.frameWidth;
    canvas.height = renderer.frameHeight;
    const image = new Image();
    let cancelled = false;
    let animationFrame = 0;
    let frameIndex = 0;
    let lastFrameAt = 0;
    let completed = false;
    let audio: HTMLAudioElement | null = null;

    const draw = (now: number) => {
      if (cancelled || !image.complete) return;
      if (!lastFrameAt) lastFrameAt = now;
      if (now - lastFrameAt >= clip.frameDurationMs) {
        const elapsedFrames = Math.max(1, Math.floor((now - lastFrameAt) / clip.frameDurationMs));
        frameIndex += elapsedFrames;
        lastFrameAt += elapsedFrames * clip.frameDurationMs;
        if (frameIndex >= clip.frames.count) {
          if (clip.mode === "loop") frameIndex %= clip.frames.count;
          else {
            frameIndex = clip.frames.count - 1;
            if (clip.mode === "once" && !completed) {
              completed = true;
              completionRef.current();
            }
          }
        }
      }
      const column = clip.frames.start + frameIndex;
      context.clearRect(0, 0, canvas.width, canvas.height);
      context.drawImage(
        image,
        column * renderer.frameWidth,
        clip.frames.row * renderer.frameHeight,
        renderer.frameWidth,
        renderer.frameHeight,
        0,
        0,
        renderer.frameWidth,
        renderer.frameHeight,
      );
      animationFrame = requestAnimationFrame(draw);
    };

    image.onload = () => {
      if (cancelled) return;
      animationFrame = requestAnimationFrame(draw);
      if (binding.sound) {
        const sound = pet.manifest.sounds[binding.sound];
        const path = pet.soundPaths[binding.sound];
        if (sound && path) {
          audio = new Audio(localAssetUrl(path));
          audio.volume = sound.volume;
          audio.loop = sound.loop ?? false;
          void audio.play().catch(() => undefined);
        }
      }
    };
    image.onerror = () => onError?.("Failed to load the selected pet spritesheet.");
    image.src = localAssetUrl(pet.sheetPath);

    return () => {
      cancelled = true;
      cancelAnimationFrame(animationFrame);
      image.onload = null;
      image.onerror = null;
      if (audio) {
        audio.pause();
        audio.removeAttribute("src");
        audio.load();
      }
    };
  }, [action, onError, pet]);

  return (
    <canvas
      ref={canvasRef}
      className="pet-canvas h-full w-full object-contain [image-rendering:auto]"
      aria-label={pet?.manifest.name ?? "Desktop pet"}
      onPointerEnter={() => onHoverChange(true)}
      onPointerLeave={() => onHoverChange(false)}
      onClick={(event) => {
        event.stopPropagation();
        onTap();
      }}
    />
  );
}
