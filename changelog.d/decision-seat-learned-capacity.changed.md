- **A decision batch is not refused twice at a size it already learned.**
  When the System One decision service refuses a request of several items
  for capacity, the rest of the batch never sends a request that large
  again: a waiting half as large as the refused request is split before it
  leaves, in order, instead of being sent whole to be refused. The seat a
  Session keeps also remembers what its batches learned, so a later
  compile's batch starts at the largest size the service answered, below
  every refused size; a batch that never met the capacity teaches nothing,
  and nothing persists beyond the process. Each journaled request split
  before it left names that bound (`split_below`). Single items, order,
  answers and the rest of the journal are unchanged.
