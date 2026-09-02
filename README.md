# Desktop Conversation Summarizer

This project is a fully local Desktop Conversation Summarizer that transcribes multi-speaker conversations in Gujarati, Hindi, and English (including Hinglish/Gujlish code-switching) and outputs structured Markdown summaries.

## Key Features

- **Multi-Speaker Transcription**: Supports transcription of conversations with multiple speakers.
- **Language Support**: Handles Gujarati, Hindi, English, and code-switching between these languages.
- **Structured Output**: Generates Markdown summaries for easy readability and organization.

## Technical Stack

- **Desktop Core**: Built with Tauri v2 (Rust) for managing audio capture and subprocess IPC.
- **Frontend**: Utilizes Vite + React + Tailwind CSS for a responsive user interface.
- **Backend Sidecar**: A bundled Python binary running:
  - `faster-whisper` for optimized transcription.
  - `pyannote.audio` for speaker diarization.
  - Integration with an Ollama LLM for enhanced summarization.

## Project Structure

```
my-tauri-app
├── src
│   ├── App.tsx
│   ├── main.tsx
│   ├── styles.css
│   └── components
│       └── recorder-panel.tsx
├── src-tauri
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── src
│   │   ├── lib.rs
│   │   ├── main.rs
│   │   ├── audio
│   │   │   ├── mod.rs
│   │   │   ├── recorder.rs
│   │   │   └── wav_writer.rs
│   │   ├── commands
│   │   │   └── audio.rs
│   │   └── state
│   │       └── app_state.rs
├── package.json
├── vite.config.ts
├── tailwind.config.js
├── tsconfig.json
├── index.html
├── .gitignore
└── README.md
```

## Setup Instructions

1. **Clone the Repository**:
   ```bash
   git clone <repository-url>
   cd my-tauri-app
   ```

2. **Install Dependencies**:
   - For the frontend:
     ```bash
     npm install
     ```
   - For the backend (Rust):
     ```bash
     cd src-tauri
     cargo build
     ```

3. **Run the Application**:
   ```bash
   npm run tauri dev
   ```

## Usage Guidelines

- Use the controls in the Recorder Panel to start and stop audio recording.
- The application will automatically transcribe the recorded audio and generate a summary.
- Access the history directory to view past recordings and summaries.

## Contribution

Contributions are welcome! Please open an issue or submit a pull request for any enhancements or bug fixes.

## License

This project is licensed under the MIT License. See the LICENSE file for details.