# Research Report: Linear -- Complete Feature Inventory (March 2026)

## Summary

Linear is a purpose-built project management tool for product development teams, offering issue tracking, project management, cycle planning, AI-powered automation, and deep integrations with developer tools. As of March 2026, it has evolved significantly with the introduction of Linear Agent, Code Intelligence, Diffs (in-app code review), Releases (CI/CD integration), Product Intelligence, Dashboards, and MCP server support. This report catalogs every documented feature across 12 categories, with pricing tier annotations for a 2-person team evaluation.

---

## 1. Issues and Tracking

### Issue Basics
- Issues belong to a single team, have a unique ID (team prefix + number), require title + status
- All other properties are optional: description, assignee, priority, labels, due dates, estimates, project, cycle, milestone
- Issue creation: keyboard shortcut (C), full screen mode, command palette, URL (linear.new), GraphQL API, email intake, integrations
- Issue creation from highlighted text auto-fills title
- Changes within first 3 minutes are grouped as "creation" (not logged as separate changes)
- Recurring issues: automated repeat on custom cadence (daily, weekly, biweekly, monthly, custom)
- Issue drafts: unsent comments visible in Drafts sidebar section

### Issue Editing
- Inline editing of title and description
- Version history for descriptions (available 10+ minutes after edit)
- Revert/restore issue description
- Move issues between teams (generates new ID, old URLs redirect)
- Bulk selection and editing (Cmd+A to select all)

### Sub-Issues (Parent/Child)
- Create sub-issues from parent issue, from comments, or from selected text
- Unlimited nesting depth
- Auto-close parent when all sub-issues complete (configurable)
- Convert comments to sub-issues
- Sub-issues can be in different teams than parent

### Issue Relations
- **Related**: auto-created when referencing issues in descriptions/comments
- **Blocked / Blocking**: orange flag (blocked by), red flag (blocks), turns green when resolved
- **Duplicate**: merge duplicate into canonical issue, marks duplicate as canceled
- All relation types are additive (multiple per issue)

### Priority Levels
- No Priority, Low, Medium, High, Urgent (4 levels + none)
- No custom priorities by design
- Priority ordering: drag and drop within priority groups, saved globally
- Urgent priority triggers immediate notification + email to assignee

### Estimates
- Scale options: Exponential (1,2,4,8,16), Fibonacci (1,2,3,5,8), Linear (1,2,3,4,5), T-Shirt (XS,S,M,L,XL)
- Extended scales available (+2 additional values: e.g. XXL, XXXL)
- Zero estimates option
- Configured per team (different teams can use different scales)
- Used in cycle/project capacity calculations and graphs
- T-shirt sizes map to Fibonacci for calculations

### Due Dates
- Color-coded calendar icon: red (overdue), orange (due within week), grey (future)
- Notifications when approaching or past due
- Filter by: overdue, 1 day, 1 week, 3 months, custom date, no due date
- Sort by due date

### Issue Templates
- **Standard templates**: workspace-level or team-level, with placeholder text formatting
- **Form templates**: structured fields (text, long text, dropdown, checkboxes), required fields support
- Templates can be used from Slack (via Asks), email intake, and directly in Linear
- Default template per team (auto-applied on new issues)
- Up to 10 templates exposed to Intercom/Zendesk integrations

### Issue Documents
- Attach documents to issues (specs, resources)
- Markdown editor with slash commands
- Document templates
- Document subscriptions (changes, comments, replies, deletions, mentions)
- Inline comments on documents (select text + comment)
- Collaborative real-time editing
- Version history and revert

### Comments and Reactions
- Threaded comments
- Emoji reactions on descriptions and comments
- Inline file attachments (paperclip, Cmd+Shift+P, drag and drop)
- Edit/delete own comments
- Create sub-issues from comments
- Copy comment URL
- Drafts for unsent comments
- Resolve comments

### SLAs (Service Level Agreements) -- BUSINESS/ENTERPRISE
- Automatic SLA assignment based on rules (priority, labels, etc.)
- Time intervals: 12h, 24h, 48h, 1 week, 2 weeks, 4 weeks
- Visual fire icon: gray > yellow > orange > red as deadline approaches
- SLA success/failure tracking with completion time
- Default rules: Urgent = 24h, High = 1 week
- Manual SLA override possible
- SLA status visible as filter/display property

### Customer Requests -- ALL PLANS
- Link customer feedback to issues or projects
- Customer pages showing all requests per customer
- Customer attributes: revenue, size, tier
- Filter issues/projects by customer name, count, status, tier, revenue, size
- Track customer requests on projects

### Triage -- ALL PLANS (rules: BUSINESS/ENTERPRISE)
- Special inbox for incoming issues (from integrations, non-team members)
- Actions: Accept (move to backlog/default status), Decline (cancel), Mark as Duplicate (merge), Snooze (hide until time/activity)
- Triage responsibility: rotating schedule of ownership (BUSINESS/ENTERPRISE)
- Triage rules: automated actions on filterable properties (BUSINESS/ENTERPRISE)
- Triage Intelligence: AI-powered property suggestions and duplicate detection (BUSINESS/ENTERPRISE)

### Bulk Actions
- Select multiple issues (Shift+click, Cmd+click, Cmd+A)
- Bulk move between teams
- Bulk change status, assignee, priority, labels, project, cycle
- Bulk archive/delete

### Delete and Archive
- **Delete**: recoverable from team archives for 30 days
- **Auto-close**: configurable time period for stale issues (Team Settings > Workflows)
- **Auto-archive**: automatic for closed issues after configurable time period
- Archive is automatic only (no manual archive button)
- Parent issues won't auto-archive until all sub-issues are closed

---

## 2. Projects and Roadmaps

### Projects
- Units of work with clear outcome/completion date (features, launches)
- Comprised of issues + optional documents
- Shared across multiple teams
- Properties: Name, Status, Lead (1 person), Team(s), Start date, Target date, Members, Icon, Milestone, External links
- View modes: List, Board, Timeline
- Attach custom issue views to projects (filtered tabs)
- Project details sidebar (Cmd+I)
- Issues can only belong to one project at a time (workaround: sub-issues in different projects)

### Project Status
- Categories: Backlog, Planned, In Progress, Completed, Canceled
- Custom status names, descriptions, colors within each category (Settings > Projects > Statuses)
- Manual status updates (not automatic even if all issues complete)
- Auto-archive when project closed + all issues closed

### Project Milestones
- Stages within a project lifecycle
- Target dates per milestone
- Descriptions per milestone
- Filter and group issues by milestone
- Reorder milestones (drag and drop)
- Convert milestone to its own project
- Visible on Initiative and team timeline views with completion percentage
- Create milestones from timeline (right-click on date)

### Project Documents
- Create specs, PRDs, status updates inside projects
- Resources section for external links
- Same Markdown editor as issues
- Document templates
- Inline comments, subscriptions, collaborative editing

### Project Graph
- Auto-generated once project reaches Started status
- Updates hourly
- Shows: Scope (gray), Started (yellow), Completed (solid blue), Completed bars (blue bars)
- Live predictions: estimated completion date based on weekly velocity
- Optimistic and pessimistic predictions (plus/minus 40%)
- Target date shown as red vertical line
- Breakdown by assignee and label

### Project Priority
- Same levels as issues: No Priority, Low, Medium, High, Urgent
- Drag and drop ordering within priority groups
- Items without priority sorted last by default

### Project Dependencies
- Blocking/Blocked-by relationships between projects
- Visual dependency lines on timeline view (blue = valid, red = violated)
- Create from contextual menu or by dragging on timeline
- Smart behavior: dragging bumps dependent projects, hold Cmd to keep in place, hold Shift to move chain together
- Filter: has dependencies, has blocking, has blocked-by, has violated dependencies

### Project Templates
- Predefined issues, milestones, status, lead, members, initiatives
- Workspace-level or team-level
- Default template per team
- Sub-issues supported in templates

### Project Labels
- Organize projects with custom labels (separate from issue labels)
- Label groups (mutually exclusive within group)
- Filter, group, and list-column display by project label
- Integrated with Insights

### Project Notifications
- Personal: new issue created, comment/change on description, issue completed/canceled, new project update
- Slack channel notifications per project
- Bell icon to subscribe

### Initiatives -- ALL PLANS (Sub-initiatives: ENTERPRISE)
- Manually curated lists of projects aligned with organizational goals
- Properties: Status (Planned, Active, Completed), Owner, Target Date, Resources, Description
- Initiative Health: On track / At risk / Off track (from latest update)
- Active Projects roll-up with color-coded health indicators
- Initiative graph: curves per project showing completion rate over time
- Workspace-wide visibility (no private initiatives)

### Sub-Initiatives -- ENTERPRISE
- Nest initiatives up to 5 levels deep
- Parent auto-includes all projects from sub-initiatives
- Multiple parents allowed
- Use for: company-wide objectives, phased delivery, grouping by quarter/theme/ownership

### Initiative and Project Updates
- Structured reports with health indicator (On track, At risk, Off track) + rich text
- Configurable reminders (weekly, biweekly, specific day/time)
- Default Slack channel for updates
- View history of all updates
- Emoji reactions on updates
- Copy as Markdown or link

---

## 3. Cycles (Sprints)

### Configuration
- Duration: 1-8 weeks (fixed interval, no custom per-cycle)
- Cooldown periods between cycles (for tech debt, planning)
- Starting day of the week
- Up to 15 upcoming cycles pre-created
- Cycle name and description editable

### Cycle Automations
- **Auto-rollover**: unfinished work automatically rolls to next cycle
- **Auto-add active issues**: configurable to add Active/Started/Completed issues without a cycle
- Cannot keep unfinished issues in a closed cycle

### Cycle Management
- Adjust start/end dates for upcoming and current cycles
- Start a cycle early ("Start cycle today")
- Cycle calendars: subscribe via Google Calendar, feed URL, or .ics file
- Cannot change past cycle dates
- Assign completed issues to previous cycles retroactively

### Cycle Capacity
- Capacity dial on upcoming cycles
- Calculated from velocity of previous 3 completed cycles
- New teams: estimated from number of members

### Cycle Graph
- Auto-generated when cycle begins, updates hourly
- Gray line: total scope
- Blue dotted line: target (even distribution over remaining days, flattens on weekends)
- Yellow line: issues started
- Solid blue line: issues completed
- Blue bars: completed issues per day
- Scope changes tracked (for scope creep visibility)

### Cycle Success
- Calculated: completed issues = 100%, started = 25%, untouched = 0%
- Example: 10 issues, 5 completed, 4 started, 1 untouched = 60%

### Cycle Sidebar
- Distribution of work across team members
- Issue/estimate count per member
- Percentage completion per member
- Click member to filter view

---

## 4. Views and Filters

### Layout Types
- **List view**: default, supports grouping and ordering
- **Board view** (Kanban): columns by status/project/priority/cycle/label/label group/SLA/Focus
- **Timeline view**: projects only, week/month/quarter/year resolution
- **Swimlanes**: sub-grouping in board view for swim-lane structure

### Custom Views
- Issue views, Project views, Initiative views (Enterprise)
- Filter, group, order, display properties all configurable
- Save and share views (workspace-level or team-level)
- View owners
- View subscriptions: personal notifications or Slack channel notifications
- Attach views to teams or projects as tabs
- Duplicate views
- Favorites: add views to sidebar

### Display Options
- **Grouping**: Status, Assignee, Project, Priority, Cycle, Focus, Label, Label group, SLA status
- **Sub-grouping**: available in lists and boards (swim-lanes)
- **Ordering**: Status, Manual, Priority, Last created, Last updated, Due date, Link count
- Reverse sort order
- **Display properties**: customizable per view (show/hide due dates, estimates, labels, etc.)
- Save as personal preference or workspace default
- "Set as default" applies to all workspace members

### Filters
- Filter by: Priority, Cycle, Estimate, Labels, Links, Project, Status, Auto-closed, Blocked, Blocking, Related, Parent, Sub-issue, Duplicate, Completed date, Created date, Due date, Updated date, Assignee, Created by, Subscribers, Content
- **Advanced Filters**: AND/OR logic with nested filter groups
- Quick filter: type shortcut to open filter menu
- Filter syntax: @-mention teams, users, status to auto-create filters
- Saved as part of custom views

### Search
- Global search: issues, projects, documents across workspace
- Search by ID (exact: LIN-123, shorthand: lin123)
- Search in: title, description, comments
- Quick shortcuts: `/` for issues, `@` for users, `#` for teams, etc.
- Find in view (Cmd+F): search within current board/list
- Recent issues history
- Search results ordered by relevance (active > backlog > completed > archived)
- Operators: quoted "exact term" search

### Peek Preview
- Preview issues without leaving current view
- Quick access to issue details

### Favorites
- Favorite views, projects, issues for sidebar access
- Favorite folders for organization
- Set favorite view as default page when opening Linear

### Label Views
- View issues grouped by label
- Navigate labels hierarchically (groups > labels)

### Team Pages
- Default team pages: All Issues, Active, Backlog, Cycles, Projects
- Customizable with attached views

---

## 5. Labels and Organization

### Issue Labels
- Workspace-level labels (shared across all teams)
- Team-level labels (team-specific)
- Create labels from settings or inline during "Add label" flow

### Label Groups
- One level of nesting (group > labels)
- Labels within groups are NOT multi-selectable (one per group per issue)
- Maximum 250 labels per group
- Create with syntax: `Type/Bug` or `Type:Bug`

### Label Management
- Edit name, color
- Merge labels
- Delete labels
- Convert to/from group
- Move between workspace/team scope
- View label metadata: SLA rules, triage rules, last applied date, issue count

### Label Descriptions and Archiving (2025)
- Add descriptions to labels for clarity
- Archive unused labels (keep for historical data, hide from active use)

### Project Labels (separate from issue labels)
- Organize projects with custom labels
- Project label groups (mutually exclusive)
- Use in Insights as a slice dimension

---

## 6. Integrations

### GitHub Integration -- ALL PLANS
- **Organization + Account connection**
- **PR linking**: auto-link via branch names, PR titles, or "magic words" (Fixes LIN-123, Closes LIN-123)
- **Commit linking**: magic words in commit messages
- **Issue status automation**: auto-update Linear status when PR is drafted/opened/merged/commits pushed
- **Branch-specific rules**: customize automation per target branch
- **Auto-assign + move forward**: assign yourself and move issue to "In Progress" when creating a branch
- **PR Reviews in Linear**: sync review state (approved, changes requested, commented)
- **PR Notifications**: review requests and activity in Linear
- **GitHub Issues Sync**: one-way or two-way sync between GitHub repos and Linear teams
- **GitHub Issues Importer**: import historical GitHub issues
- **Git branch auto-naming**: customizable branch format in settings
- **Code Intelligence (Beta)**: ask questions about your codebase from Linear (BUSINESS/ENTERPRISE)
- **Diffs (Beta)**: review code changes directly in Linear, approve/request changes/merge from Linear

### Slack Integration -- ALL PLANS
- @Linear mention in Slack to create issues or ask questions
- Create issues from Slack messages
- Sync threads between Slack and Linear (bidirectional comments)
- Rich unfurls (issue, project, comment, document details)
- Personal, team, and project Slack notifications
- Channel-specific notifications
- Linear Asks integration (creates issues from Slack requests)
- Linear Agent for Slack (AI agent responding to Slack interactions)

### Discord Integration -- ALL PLANS
- `/linear issue` to create issues
- `/linear search` to search and post issues
- `/linear wrap` to post daily work summary
- Link Discord messages to Linear issues

### Figma Integration -- ALL PLANS
- Embed Figma previews in Linear issues/comments/documents
- Linear plugin for Figma (create/link issues from Figma)
- Interactive in-app preview for public files
- Refresh previews on demand

### Sentry Integration -- ALL PLANS
- Create Linear issues from Sentry exceptions
- Link Sentry issues to existing Linear issues
- Auto-resolve Sentry issues when Linear issue completes
- Auto-sync assignee changes
- Configure automatic issue creation from Sentry alerts
- Display Sentry icon on issue views

### GitLab Integration -- ALL PLANS
- Merge request linking (hosted and self-hosted)
- Status automation on MR draft/open/merge/review
- Branch-specific rules
- Webhook-based (requires GitLab 15.6+)

### Intercom Integration -- BUSINESS/ENTERPRISE
- Create/link Linear issues from Intercom conversations
- Issue status and assignee visible in Intercom sidebar
- Use templates (up to 10)
- Create with Linear Agent (AI-powered issue creation from conversation)
- Re-open conversations when issues complete

### Zendesk Integration -- BUSINESS/ENTERPRISE
- Create/link Linear issues from Zendesk tickets
- Issue status and assignee visible in Zendesk
- Create with Linear Agent (AI-powered)
- Auto-reopen tickets when issues complete
- Use templates (up to 10)

### Jira Integration -- ALL PLANS
- **Jira Sync**: bidirectional sync between Jira spaces and Linear teams (forward-looking)
- **Jira Import**: one-time import of existing issues and projects
- Supports Jira Cloud and Server (with PAT)
- Comment sync between platforms

### Salesforce Integration -- ENTERPRISE (add-on)
- Create/link issues from Salesforce cases
- Real-time status and priority updates
- Permission sets: Admin, Create Issues, Link Only
- Restrict issue visibility to Salesforce-linked issues only

### Notion Integration -- ALL PLANS
- Embed Linear issues and projects in Notion pages
- Rich previews that auto-refresh
- Multiple Notion workspaces can connect to one Linear workspace

### Google Sheets Integration -- ALL PLANS
- Auto-generated spreadsheet of workspace issue/project data
- Hourly refresh
- Syncs: team, title, description, status, estimate, priority, project, creator, assignee, labels, cycle, due date, SLA status, and more
- Projects and Initiatives sheets also available

### Front Integration -- BUSINESS/ENTERPRISE
- Create/link issues from Front conversations
- Linked issue status in Front sidebar
- Auto-reopen Front conversations on issue completion

### Gong Integration -- BUSINESS/ENTERPRISE
- Linear Agent integration for Gong

### Microsoft Teams Integration -- ALL PLANS
- Create issues and receive notifications

### Airbyte Integration -- ALL PLANS
- Data warehouse sync

### Zapier Integration -- ALL PLANS
- **Actions**: Create issue, Update issue, Create attachment, Create comment, Create project
- **Triggers**: New issue, New comment, New document comment, New project, New project update, Updated issue, Updated project update
- Build custom no-code automations

### API and Webhooks -- ALL PLANS
- **GraphQL API**: full read + write access, same API used internally
- **Personal API keys**: configurable permissions (Read, Write, Admin, Create issues, Create comments)
- **Team-scoped API keys**
- **Webhooks**: Issues, Comments, Attachments, Documents, Emoji reactions, Projects, Project updates, Cycles, Labels, Users, Issue SLAs
- **Webhook events**: create, update, delete for all supported entities
- **TypeScript SDK** with strongly typed models
- **OAuth 2.0 / 2.1** authentication
- **Rate limiting**: documented

### MCP Server -- ALL PLANS
- Model Context Protocol server for AI agents
- Supports: Claude (Desktop, Code, Team, Enterprise), Cursor, Codex, other MCP-compatible clients
- Tools for: finding, creating, updating issues, projects, comments
- OAuth 2.1 with dynamic client registration
- URL: `https://mcp.linear.app/mcp`

---

## 7. Automations

### Built-in Issue Automations
- **Auto-close**: close stale issues after configurable time period
- **Auto-archive**: archive closed issues after configurable time
- **Auto-close parent**: when all sub-issues complete
- **Auto-assign to cycle**: add active/started/completed issues to current cycle
- **Cycle auto-rollover**: unfinished issues move to next cycle
- **SLA auto-assignment**: based on priority/label rules
- **Triage rules**: automated actions when issues enter triage (BUSINESS/ENTERPRISE)

### GitHub/GitLab Workflow Automations
- Auto-update issue status when PR is drafted, opened, merged
- Auto-update status on commit push
- Auto-assign self when creating branch
- Auto-move issue to "In Progress" on branch creation
- Auto-resolve Sentry issues on issue completion
- Branch-specific automation rules

### Linear Agent Automations -- BUSINESS/ENTERPRISE (Beta)
- AI-powered automations that can create issues, update properties, post comments
- Workspace and team-level automation rules
- Available across Linear Agent chat, comments, and integrations

### Triage Intelligence -- BUSINESS/ENTERPRISE
- LLM-powered property suggestions (team, project, assignee, label)
- Duplicate and relationship detection
- Auto-apply suggestions (configurable per team)
- Additional guidance to refine behavior
- Processing time: 1-4 minutes per issue

---

## 8. Documents

### Issue Documents
- Attach documents to issues
- Specs, additional resources
- Markdown editor with full formatting

### Project Documents
- Create inside projects (Resources section)
- Specs, PRDs, status updates
- Same editor as issues

### Document Templates
- Workspace-level and team-level document templates
- Used when creating new documents in projects or issues

### Editor Features
- Full Markdown support (auto-converts pasted Markdown)
- Slash commands (/, headers, lists, code blocks, dividers, blockquotes)
- Text styling: bold, italic, strikethrough, underline, inline code
- Headers: H1, H2, H3
- Lists: bulleted, numbered, checklists
- Code blocks
- Blockquotes
- Dividers
- Collapsible sections
- Image embedding and resizing
- Video uploads and player
- File attachments
- @mentions (users, issues, projects, documents)
- Inline comments (select text + comment)
- Collaborative real-time editing
- Version history
- Link to headers (copy heading URL)
- External URL references

### Document Subscriptions
- Subscribe to specific documents
- Notification types: changes, comments, replies, deletions, mentions
- Auto-subscribe document creator

---

## 9. Analytics and Insights

### Insights -- BUSINESS/ENTERPRISE
- Real-time analytics on Linear issue data
- Available in custom views, team views, project views, cycle views
- **Measure**: issue count, issue age, triage time, etc.
- **Slice**: any issue property (assignee, label, project, priority, etc.)
- **Segment**: color-coded additional dimension
- **Formats**: chart, table, single metric
- **Filters**: Created at, Completed at, Status Type, Label, Project, Team
- Include/exclude archived issues
- In-app Help Center with example insights

### Dashboards -- ENTERPRISE
- Combine insights from across teams and projects
- Dashboard-level filters (apply globally to all insights)
- Insight-level filters (per insight)
- Formats: chart, table, single metric
- Click into charts/metrics to view underlying issues
- Create from Views page or from existing insights

### Cycle Graph
- Auto-generated, hourly updates
- Scope, started, completed lines
- Target line (even distribution, flattens on weekends)
- Cycle success percentage

### Project Graph
- Auto-generated once Started status
- Scope, started, completed lines with predictions
- Breakdown by assignee and label
- Weekly velocity-based predictions with optimistic/pessimistic range

### Initiative Graph
- Curves per project showing completion rates
- Hover for weekly activity breakdown

### Google Sheets Export
- Hourly auto-refresh of issue, project, initiative data
- Build custom analytics externally

### Data Export
- CSV export (workspace admins)
- Google Sheets integration
- API access for custom analytics

---

## 10. AI Features

### Linear Agent (Beta) -- ALL PLANS (basic), BUSINESS/ENTERPRISE (automations)
- Lives inside Linear, understands workspace context
- **Capabilities**:
  - Create and update issues, projects, milestones, initiatives
  - Summarize and analyze ongoing work, threads, customer requests
  - Answer questions about workspace data
  - Post, edit, delete comments in threads
  - Draft documents and updates
- **Access**: Agent chat (Ctrl+Space), @Linear mention in any comment, inline in project/initiative descriptions
- **Chat features**: multiple concurrent chats, chat history, grouped by recency
- All plans include base usage; some capabilities may move to usage-based pricing

### Code Intelligence (Beta) -- BUSINESS/ENTERPRISE
- Analyze connected GitHub repositories
- Ask questions about implementation, architecture, history
- Links to relevant files, commits, PRs
- Powered by Claude Code
- Permission-aware repository access
- Optional "extend access to all members"
- Custom Claude Code guidance for workspace conventions

### Triage Intelligence -- BUSINESS/ENTERPRISE
- AI-powered issue property suggestions (assignee, team, project, label)
- Duplicate and relationship detection based on semantic similarity
- Auto-apply configurable per team
- Reasoning visible (why suggestions were made)
- Additional guidance to refine behavior
- Processing: 1-4 minutes per issue

### Issue Discussion Summaries (2025)
- AI-generated summaries of issue discussions

### AI Filters (2023)
- Use natural language to create filters

### Similar Issues Detection
- Surface similar/duplicate issues when creating new ones

### Product Intelligence (Technology Preview, 2025) -- BUSINESS/ENTERPRISE
- Broader AI analytics capabilities (evolving)

### Agent Platform (2025)
- Agent Interaction Guidelines (AIG)
- Agent SDK
- Agents for: Cursor, GitHub Copilot, OpenAI Codex, Slack
- Custom agent development support
- Agent guidance (workspace-level and team-level instructions)
- Delegation model: agent acts on issue while human maintains ownership

---

## 11. Less Known / Power User Features

### Keyboard Shortcuts (Extensive)
- C: create issue, Shift+C: full screen create
- D: set due date
- E: set estimate
- L: add label
- P: set priority
- A: assign to self
- Shift+A: assign to someone
- Cmd+K: command palette
- Cmd+/: open search
- /: filter menu
- Cmd+Shift+P: move to project
- Cmd+Shift+M: set milestone
- O then W: switch workspace
- And dozens more...

### Command Palette (Cmd+K)
- Universal command menu
- Search and execute any action
- Create issues, navigate, change properties
- Filter by typing: `/` issues, `@` users, `#` teams, `*` labels, `^` favorites, `~` documents

### Inbox
- Central notification hub
- Grouped by type: assigned, mentioned, subscribed
- Snooze notifications
- Mark as read/unread
- Pulse summaries delivered to inbox

### My Issues
- Personal view of assigned issues
- Focus grouping (AI-organized by what to work on first)
- Shared tab (issues shared from private teams)
- Delegated issues still visible

### Pulse -- ALL PLANS
- Feed of project and initiative updates
- Tabs: For me, Popular, Recent
- Daily or weekly summaries to Inbox
- Custom feeds with filters
- Pulse audio: hear summarized updates read aloud (desktop, web, mobile)
- Workspace-level feature

### Pull Request Reviews -- ALL PLANS (with GitHub)
- PR review state synced into Linear
- See reviewers and review status on linked issues
- PR notifications in Linear
- Reviews sidebar section

### Linear Diffs (Beta) -- ALL PLANS (with GitHub)
- View pull request diffs directly in Linear
- Unified and Split diff views
- Inline comments (bidirectional with GitHub)
- Approve, request changes, submit review from Linear
- Merge PRs from Linear
- Reviews sidebar with "For me" and "Created" tabs
- CI check status visible
- Notifications for PR activity

### Releases (Beta) -- BUSINESS/ENTERPRISE
- Connect CI/CD to Linear
- Release pipelines: continuous or scheduled cadence
- Release = commit SHA + associated issues
- Semantic versioning auto-increment
- Path filters (monorepo support)
- Filter issues by release, stage, pipeline
- Business: 5 pipelines, Enterprise: unlimited

### Linear Asks -- BUSINESS/ENTERPRISE (Advanced: ENTERPRISE)
- Turn workplace requests into issues
- **Slack intake**: create from Slack messages, bidirectional thread sync
- **Email intake**: custom email addresses, confirmation emails, synced replies
- **Form templates**: structured forms in Slack and web
- **Asks Web Forms (Beta, Enterprise)**: web portal for employees, SAML auth, no Linear account needed
- **Per-channel configurations** (Enterprise)
- **Private Asks** via DMs (Enterprise)
- **Multiple Slack workspaces** (Enterprise)
- **Auto-create on emoji reaction** (Enterprise)
- **Auto-create on new message** (Enterprise)

### Private Teams
- Restrict team visibility to members only
- Guest accounts for external collaborators
- Private issue sharing (Beta): share individual issues from private teams with specific users

### Sub-Teams (2025)
- Nest teams hierarchically
- Sub-team settings can inherit from or override parent

### Team Owners (2025) -- BUSINESS/ENTERPRISE
- Designated team owner role with additional permissions

### Notifications System
- In-app (Inbox), Email, Slack, Mobile push
- Configurable per type (assigned, mentioned, subscribed, project updates, due dates)
- Snooze notifications
- View subscriptions (notify on issue added/completed)

### Drafts
- Unsent comments saved as drafts in sidebar
- Resume editing later

### Git Branch Auto-Naming
- Customizable branch format in settings
- Auto-create branch names from issue ID and title

### PR Auto-Linking
- Magic words in PR descriptions: "Fixes LIN-123", "Closes LIN-123", "Resolves LIN-123"
- Branch name matching (branch contains issue ID)
- PR title matching

### Recurring Issues
- Create issues on automated cadence
- Convert existing issues to recurring

### Configuring Workflows (Issue Status)
- Customizable workflow states per team
- Categories: Triage, Backlog, Unstarted, Started, Completed, Canceled
- Custom status names and descriptions within categories
- Multiple statuses per category allowed

### Import and Export
- **Import from**: Jira, GitHub Issues, Asana, Clubhouse/Shortcut, Pivotal Tracker, Trello, CSV
- **Export**: CSV (admin), Google Sheets (hourly sync), API

### Time in Status (2026)
- Track how long issues spend in each status

### Mobile App (iOS + Android, 2024 redesign)
- Full issue management
- Pulse on mobile
- Customizable navigation

### Desktop App
- Windows, macOS, Linux
- Multi-window support

### Passkeys (2024)
- Passwordless login via passkeys

### Personalized Sidebar (2024)
- Customize sidebar items, visibility, and ordering

### Custom Emojis
- Upload custom emojis for reactions

---

## 12. Pricing and Limits (March 2026)

### Free Plan
- **Members**: Unlimited
- **Issues**: 250
- **Teams**: 2
- **File upload**: 10 MB per file
- **Core features**: Issues, projects, cycles, initiatives, customer requests, API/webhooks, import/export, triage, Pulse
- **SSO**: Google only
- **Agent platform**: yes
- **MCP access**: yes
- **Linear Agent (beta)**: yes
- **Integrations**: yes (except support integrations)
- **All users are Admins** (no role distinction)
- **Note**: No issue sync, no SLAs, no triage responsibility/rules, no sub-initiatives

### Basic Plan -- $10/user/month (annual)
- All Free features plus:
- **Teams**: 5
- **Issues**: Unlimited
- **File upload**: Unlimited
- **Admin roles**: yes
- **Issue sync**: yes
- **Sub-teams**: yes

### Business Plan -- $16/user/month (annual)
- All Basic features plus:
- **Teams**: Unlimited
- **Private teams and guests**: yes
- **Triage Intelligence**: yes
- **Linear Agent automations (beta)**: yes
- **Insights**: yes
- **Issue SLAs**: yes
- **Triage responsibility and rules**: yes
- **Linear Asks** (Slack + email intake)
- **Support integrations**: Intercom, Zendesk, Front
- **Team owners**: yes
- **Progress reports**: yes
- **Releases**: up to 5 pipelines
- **Code Intelligence (beta)**: yes

### Enterprise Plan -- Custom pricing (annual only)
- All Business features plus:
- **Sub-initiatives**: yes
- **Dashboards**: yes
- **Advanced Linear Asks**: multiple Slack workspaces, private channels, per-channel configs, auto-create, web forms
- **SAML and SCIM**: yes
- **Initiative views** (custom): yes
- **Audit log**: yes
- **IP restrictions**: yes
- **Domain claiming**: yes
- **Third-party app management**: yes
- **HIPAA compliance** (with BAA)
- **Workspace owner role**: yes
- **Advanced authentication**: yes
- **Data warehouse sync**: yes
- **Salesforce integration** (add-on)
- **Releases**: unlimited pipelines
- **Priority support, account manager, custom terms, uptime SLA**

### Data Regions
- United States or European Union (selected at workspace creation, permanent)

### Certifications
- SOC 2 Type II, GDPR, HIPAA (Enterprise with BAA)

---

## Recommendation for a 2-Person Team

### What to Use (Free Plan is sufficient for starting)
- **Issues + sub-issues**: core workflow
- **Projects**: for features/epics
- **Cycles**: if you want sprint-like cadence (optional for 2 people)
- **Labels**: Bug, Feature, Improvement, etc. at workspace level
- **Priority**: keep it simple with No Priority / High / Urgent
- **GitHub integration**: PR linking + status automation is essential
- **Keyboard shortcuts**: massive productivity gain
- **Command palette (Cmd+K)**: fastest way to do everything
- **Board view**: for visual Kanban workflow
- **Custom views**: save filtered views for common tasks
- **MCP server**: connect to Claude Code for AI-assisted project management
- **Linear Agent**: ask questions about your workspace, create/update issues via conversation

### What to Skip (Overkill for 2 People)
- **SLAs**: no external customers to track SLAs for yet
- **Triage**: only useful when non-team members create issues
- **Triage Intelligence**: business plan, not needed at your scale
- **Initiatives**: organizational hierarchy for larger teams
- **Sub-initiatives**: enterprise, definitely overkill
- **Dashboards**: enterprise only, custom views suffice
- **Insights**: business plan, manual review is fine for 2 people
- **Cycle capacity/velocity**: meaningful with larger teams
- **Linear Asks**: internal request management for larger orgs
- **Releases**: CI/CD integration, nice but not essential early
- **Multiple teams**: start with 1 team
- **Salesforce/Intercom/Zendesk**: enterprise support integrations
- **SAML/SCIM/Audit log**: enterprise security features
- **Private teams**: no need with 2 people
- **Form templates**: overkill for internal use

### Plan Recommendation
- **Start with Free**: 250 issues is enough to evaluate
- **Upgrade to Basic ($20/month for 2 users)**: when you hit 250 issues or need unlimited file uploads
- **Business ($32/month for 2 users)**: only if you specifically need SLAs, Insights, or support integrations
- The Free plan gives you almost everything a 2-person team needs

---

## Sources

1. https://linear.app/docs/* -- Official Linear documentation (100+ pages scraped)
2. https://linear.app/pricing -- Pricing and feature comparison table
3. https://linear.app/changelog -- 150+ changelog entries (2019-2026)
4. https://linear.app/features -- Feature overview page
5. https://linear.app/method -- Linear Method (practices)
6. https://linear.app/llms.txt -- Complete documentation structure
7. https://developers.linear.app -- API documentation and GraphQL schema
8. https://linear.app/sitemap.xml -- Full sitemap for documentation discovery

## Methodology
- Tools used: curl + HTML parsing (Linear docs are SSR, required text extraction from HTML)
- Pages analyzed: ~80 documentation pages, pricing page, changelog (150+ entries), developer docs
- Data extracted from: llms.txt (full doc structure), sitemap.xml (all URLs), individual doc pages, pricing comparison table
- Time period covered: Feature set as of March 2026, with changelog history back to 2019
- Cross-referenced: pricing table features against individual doc pages for accuracy

## Confidence Level
**High** -- Data sourced directly from Linear's official documentation, pricing page, and changelog. All features verified against multiple documentation pages. Pricing data extracted from the live pricing page. Minor gaps may exist in very recent beta features not yet fully documented.
