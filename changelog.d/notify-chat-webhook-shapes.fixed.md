- **`nika:notify` now reaches Slack and Discord webhooks.** A Slack incoming webhook
  (`hooks.slack.com/services/…`) refused the stdlib body `{ message, severity, data? }`
  with HTTP 400 `no_text`, so a project whose last step posts to Slack failed on every
  run. Slack incoming webhooks now get `{ text, mrkdwn: false }` and Discord webhooks
  (`discord.com/api/webhooks/{id}/{token}`) get `{ content, allowed_mentions }`: the
  message, a non-`info` severity and the `data:` context as plain text. Slack's `&`, `<`
  and `>` become entities. Discord's markdown and mention openers are escaped, no
  mention is parsed, and the content is bounded to Discord's 2,000 characters. Every
  other target, including a Slack Workflow Builder trigger, keeps the stdlib body. A
  refused delivery (`NIKA-BUILTIN-NOTIFY-002`) now quotes the receiver's answer, as one
  bounded line with the target's own parts withheld, and carries `details.status_code`.
