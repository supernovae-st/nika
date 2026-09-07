// The review is inserted into the workflow timeline, after reading the source.
export const FILE_REVIEW = { at: 25.5, duration: 8 };
export const SOURCE_FILM = {
  duration: 96 + FILE_REVIEW.duration,
  check: 43.2 + FILE_REVIEW.duration,
  run: 48.2 + FILE_REVIEW.duration,
  result: 84 + FILE_REVIEW.duration,
  resultFrame: 89 + FILE_REVIEW.duration,
};
// Source choreography stays continuous. Only editorial pacing changes.
// [source seconds, delivered seconds]: source/review/folding get the largest cuts.
export const CUTS = [
  [0, 0], [14, 11], [25, 16], [33.5, 19], [41.5, 21.5],
  [47, 23], [51.2, 25], [56.2, 28], [64, 32], [75, 38],
  [80, 40.5], [84.25, 42.5], [88, 45.5], [92, 48],
  [101, 57.5], [104, 60],
];
export function editTime(source) {
  const i = CUTS.findIndex(([end]) => end >= source);
  if (i <= 0) return i === 0 ? 0 : 60;
  const [a, start] = CUTS[i - 1], [b, end] = CUTS[i];
  return start + (source - a) / (b - a) * (end - start);
}
export const FILM = {
  duration: 60,
  check: editTime(SOURCE_FILM.check), run: editTime(SOURCE_FILM.run),
  result: editTime(SOURCE_FILM.result), resultFrame: editTime(SOURCE_FILM.resultFrame),
};
export const CHAPTERS = [
  ['Intent', 0], ['Contract', 19], ['YAML', 28], ['Review', 38],
  ['Plan', 50], ['Context', 57], ['Parallel', 66], ['Rules', 78],
  ['Approval', 86], ['Result', SOURCE_FILM.resultFrame],
].map(([title, time]) => [title, editTime(time)]);
