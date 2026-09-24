A Session on a priced route with no budget no longer spends invisibly. Every
call it makes (conversation, labels, routing, authoring, revisions) is observed
without any allowance or cap: `.nika/session-state.json` says a request may have
been sent before it leaves, then keeps its usage and catalog estimate, or says
its charge is unknown. `/status` shows the no-budget observation, and a later
explicit budget never counts those calls as covered. The chosen model still
answers with no ceremony, but each call is now bounded (an explicit output
limit, one attempt, no automatic retry or redirect), and a reply whose usage
contradicts the catalog is refused. A restart names an interrupted call,
replays nothing and asks for no reconfirmation.
