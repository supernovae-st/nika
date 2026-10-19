- **A run says whether each step that sends sent.** The run card named the files a run produced,
  but not whether its webhook or notification went out. Each step that sends beyond the machine
  (a notification, a request other than GET) now reads « sent · `ping` · the step completed »,
  or « not sent · `ping` · its condition was false » (cancelled, failed, served from the cache),
  in the person's language.
