# We stopped writing prompts and started writing workflows

For a year our team's most valuable automation lived in a chat scrollback.
Someone had found the right way to turn a support queue into triaged tickets,
and the way you re-ran it was to scroll up, find the message, copy it, and
paste it again with new data. It worked. It was also invisible, unversioned,
and impossible to hand to anyone else.

The fix was not a better prompt. It was giving the prompt a file.

A workflow file states four things a chat message cannot: what it reads, what
it may reach, what shape the answer has to be, and what it costs. That last
one changed the conversation with our finance lead more than anything else we
shipped last quarter — she stopped asking "what is this AI spend" and started
reading a per-run number that was capped before the run started.

The part we did not expect: writing the boundary down made the work *smaller*.
When you have to name the two files a job touches, you notice that the job you
described touches nine. Half our automations shrank the week we wrote their
permits block.

None of this is about the model. We swapped ours twice while the files stayed
identical.
