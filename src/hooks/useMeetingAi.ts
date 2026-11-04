import { useEffect } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { meetingAi } from "@/ipc/meetingAi";
import { onNamed } from "@/ipc/namedEvents";

export const aiKeys = {
  job: (meetingId: string | null) => ["meeting-ai", "job", meetingId] as const,
  transcript: (meetingId: string | null) => ["meeting-ai", "transcript", meetingId] as const,
};

/** The latest transcription job of a meeting; updates live from `meeting://job`. */
export function useMeetingJob(meetingId: string | null) {
  const qc = useQueryClient();
  useEffect(
    () => onNamed("meeting://job", (job) => {
      if (job.meeting_id !== meetingId) return;
      qc.setQueryData(aiKeys.job(meetingId), job);
      if (job.status === "completed" || job.status === "partial_failure") void qc.invalidateQueries({ queryKey: aiKeys.transcript(meetingId) });
    }),
    [meetingId, qc],
  );
  return useQuery({ queryKey: aiKeys.job(meetingId), queryFn: () => meetingAi.jobGet(meetingId ?? ""), enabled: meetingId !== null });
}

export function useTranscript(meetingId: string | null, enabled: boolean) {
  return useQuery({
    queryKey: aiKeys.transcript(meetingId), queryFn: () => meetingAi.transcriptGet(meetingId ?? ""), enabled: enabled && meetingId !== null,
  });
}
