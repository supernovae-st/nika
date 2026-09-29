The Run terminal receipt and new prepared/settled cost-journal observations now
use the provider-owned durable projection: route origins replace full endpoints,
while accounting fields keep their meaning. An unreadable projection is reported
or refused, never replaced by the exact private observation. In-memory route
identity and consent are unchanged. Existing history, its derived observations
and reconciliations, and Session history are not migrated by this change.
