- **The observations of one message run side by side.** An observation reads one public
  page outside the Session's lock, and consecutive ones run at once: three sources cost
  about one page's time. Their replies keep the message's order; every other call stays
  sequential.
