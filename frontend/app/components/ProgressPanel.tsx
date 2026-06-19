"use client";

import { useEffect, useRef, useState } from "react";
import type { AuditReport } from "./ReportView.types";

interface PageDoneEvent {
  type: "PageDone";
  url: string;
  score: number;
  queue_depth: number;
  pages_done: number;
}

interface DoneEvent {
  type: "Done";
  report: AuditReport;
}

interface ErrorEvent {
  type: "Error";
  message: string;
}

type ProgressEvent = PageDoneEvent | DoneEvent | ErrorEvent | { type: string };

interface Props {
  jobId: string;
  onDone: (report: AuditReport) => void;
  onError: (message: string) => void;
}

export default function ProgressPanel({ jobId, onDone, onError }: Props) {
  const [pagesDone, setPagesDone] = useState(0);
  const [queueDepth, setQueueDepth] = useState(0);
  const [runningScore, setRunningScore] = useState<number | null>(null);
  const esRef = useRef<EventSource | null>(null);

  useEffect(() => {
    const es = new EventSource(`/api/audit/stream?job=${encodeURIComponent(jobId)}`);
    esRef.current = es;

    const handleMessage = (e: MessageEvent) => {
      let event: ProgressEvent;
      try {
        event = JSON.parse(e.data as string) as ProgressEvent;
      } catch {
        return;
      }

      if (event.type === "PageDone") {
        const pde = event as PageDoneEvent;
        setPagesDone(pde.pages_done);
        setQueueDepth(pde.queue_depth);
        setRunningScore(pde.score);
      } else if (event.type === "Done") {
        es.close();
        onDone((event as DoneEvent).report);
      } else if (event.type === "Error") {
        es.close();
        onError((event as ErrorEvent).message);
      }
    };

    // Attach per-event-name listeners
    es.addEventListener("PageDone", handleMessage);
    es.addEventListener("Done", handleMessage);
    es.addEventListener("Error", handleMessage);

    es.onerror = () => {
      es.close();
      onError("Lost connection to audit stream.");
    };

    return () => {
      es.close();
    };
  }, [jobId, onDone, onError]);

  return (
    <div className="progress-panel">
      <p className="loading-msg">Audit in progress…</p>
      <div className="progress-stats">
        <span>Pages done: <strong>{pagesDone}</strong></span>
        {queueDepth > 0 && <span>In queue: <strong>{queueDepth}</strong></span>}
        {runningScore !== null && (
          <span>Current score: <strong>{runningScore}/100</strong></span>
        )}
      </div>
    </div>
  );
}
