# Clavis Privacy Policy

Version of 24 November 2026, second revision.

Clavis keeps your clipboard history, snippets and notes on your computer in encrypted form. The developer cannot read them: only you hold the keys. This policy explains what information does reach the Clavis server, why it is needed, how long it is kept and how to delete it.

## 1. Who is responsible for your data

The controller within the meaning of Regulation (EU) 2016/679 (GDPR) is Anton Shevchenko, a private individual.

Contact for any question about your data: veritasmogger@gmail.com.

## 2. What stays on your computer only

Clipboard history, snippets, notes, passwords of secret notes, the sign-in log, settings and the recovery phrase are stored only on your computer. They are encrypted with XChaCha20-Poly1305; the key is protected by your PIN and the computer's TPM module. This data is never sent to the server.

Text recognition in images and Windows Hello sign-in are performed by Windows itself on your computer. Clavis does not pass this data to the developer or to anyone else.

Snippet abbreviations are matched on your computer: Clavis compares what you type with the list of abbreviations and sends it nowhere.

## 3. What is stored on the server and why

| Data | Purpose | Legal basis |
|---|---|---|
| A random device identifier and its public keys | Signing the device's requests and delivering keys of shared notes | Performance of the terms of use (Art. 6(1)(b) GDPR) |
| A SHA-256 hash of the computer identifier (SMBIOS UUID) | Binding an access code to one computer and blocking the computer after 15 wrong codes | Legitimate interest: protection against code guessing (Art. 6(1)(f)) |
| The Windows computer name | Showing the service owner which device was activated | Legitimate interest: access management (Art. 6(1)(f)) |
| IP address: the latest one for each device, and the one used for a failed activation or an access request | Service security, detecting abuse, reviewing access requests | Legitimate interest (Art. 6(1)(f)) |
| A hash of the IP address and the number of requests per hour | Limiting the number of requests from one address | Legitimate interest (Art. 6(1)(f)) |
| The display name you entered yourself | Showing it to co-authors of shared notes | Performance of the terms (Art. 6(1)(b)) |
| Name and message in an access request | Letting the service owner decide whether to issue a code | Steps taken at your request before entering into the agreement (Art. 6(1)(b)) |
| Time of activation and of the latest contact with the server | Access management | Legitimate interest (Art. 6(1)(f)) |
| Shared notes, snippet sets, images and comments in them | Synchronisation between members | Performance of the terms (Art. 6(1)(b)) |

Shared notes are encrypted on the members' computers before they are sent. The server stores only encrypted data, the list of members and the times of changes. Neither the server nor the developer can read the content.

The information in the table, including IP addresses, is visible only to the service owner in the owner panel.

When Clavis checks for and downloads updates, the storage that serves the update files receives your IP address and technical details of the request, like any web server.

## 4. What Clavis does not collect

Clavis does not collect the content of your clipboard, notes or snippets, keystrokes, the list of installed programs, the Windows user name, MAC address, location or screenshots. Clavis has no advertising, analytics, trackers or cookies.

## 5. Outside services you turn on yourself

Two features work through third-party services. Clavis asks for your permission before the first use, and you can withdraw it in the settings. The text is sent directly from your computer to the service; the developer of Clavis does not receive it.

- Translation: the MyMemory service of Translated S.r.l. (Rome, Italy). It receives the text you translate. Under its terms, submitted texts may be stored and used by the company for statistics and to improve its services. Do not translate passwords or personal data this way.
- Short links: the clck.ru service of Yandex (Russia). It receives the link you shorten. Russia is outside the EU and there is no European Commission decision on the adequacy of its data protection. The link is therefore sent only with your explicit consent (Art. 49(1)(a) GDPR), after Clavis has warned you about this.

These services process data under their own rules.

## 6. Who else processes data

The Clavis server runs on the Supabase platform (Supabase, Inc.) in the EU region: Frankfurt, Germany. Supabase processes data on behalf of the developer under a data processing agreement. To operate the platform, data may be transferred to the United States and Singapore under the EU Standard Contractual Clauses (Art. 46 GDPR).

Data is not sold and is not disclosed to anyone else unless the law requires it.

## 7. How long data is kept

- Device data (identifier, keys, computer hash, computer name, latest IP address, display name): while access is active. After access is revoked, 12 months, then deleted. If you asked to delete your data yourself, it is deleted 48 hours after the request.
- Records of failed activation attempts: 12 months from the latest attempt. If a computer is blocked after 15 errors, the record is kept until the block is lifted.
- IP address hashes for rate limiting: 2 hours.
- Access requests: 30 days after review; unreviewed requests, 90 days.
- Shared notes and snippet sets: until the owner of the note or set deletes it. Images: until every member has received them, but no longer than 30 days. Unused invitations: until they expire, no longer than 8 days.

## 8. Your rights

You have the right to:

- find out what data about you is processed and receive a copy of it;
- have inaccurate data corrected;
- have your data erased;
- restrict processing;
- object to processing based on legitimate interest;
- receive your data in a machine-readable format;
- withdraw your consent to sending text to outside services at any time.

To exercise a right, write to veritasmogger@gmail.com. You will receive an answer within one month. To make sure the request comes from you, we may ask you to confirm that you use the device concerned.

You can also delete all your data without writing: in Clavis open Settings → Delete all my data. Clavis data on your computer is deleted at once, and the device loses access to the server at once. The device data and the shared notes you own are deleted from the server 48 hours after the request.

If you believe your data is processed unlawfully, you can lodge a complaint with the supervisory authority of your country. In Bulgaria this is the Commission for Personal Data Protection: 2 Prof. Tsvetan Lazarov Blvd., Sofia 1592, kzld@cpdp.bg, www.cpdp.bg.

## 9. Automatic blocking

After 15 wrong access codes, Clavis automatically blocks the computer so that codes cannot be guessed. If you believe the block is a mistake, write to veritasmogger@gmail.com: the service owner will review it and can lift it.

## 10. Security

Local data is encrypted with XChaCha20-Poly1305; the key is derived from your PIN with Argon2id and protected by the TPM module. Every request to the server is signed with the device key. Shared notes use end-to-end encryption, and the key changes when a member is removed. Access to the server database is closed to everyone except the server itself.

## 11. Age

Clavis is intended for people aged 16 and over. If you are under 16, do not use Clavis.

## 12. Changes to this policy

Clavis will announce a new version of this policy at launch and ask you to confirm that you have read it. The current version is always available in the Clavis settings and in the project repository.

This policy is written in Russian and English. Both versions are equally valid.
