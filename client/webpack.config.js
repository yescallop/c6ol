import path from 'path';
import HtmlWebpackPlugin from 'html-webpack-plugin';
import CopyWebpackPlugin from 'copy-webpack-plugin';
import MiniCssExtractPlugin from 'mini-css-extract-plugin';
import CssMinimizerPlugin from 'css-minimizer-webpack-plugin';
import crypto from 'crypto';
import fs from 'fs';
import { spawn } from 'child_process';

const srcDir = path.join(import.meta.dirname, 'src');

// The app is always served from the root. `output.publicPath` is left at
// `auto`, because a static one makes webpack drop the wasm asset module
// from unconcatenated (development) builds.
const publicPath = '/';

// The icon is hashed by hand, because the manifest has to reference it
// under its final name. The digest length matches webpack's default.
function iconAssetName() {
  const content = fs.readFileSync(path.join(import.meta.dirname, 'assets/icon.svg'));
  const hash = crypto.createHash('md5').update(content).digest('hex').slice(0, 20);
  return `assets/icon-${hash}.svg`;
}

// Rebuilds the crate when its sources change, so that `webpack serve` picks
// up the new `pkg` output.
class BuildRustPlugin {
  apply(compiler) {
    compiler.hooks.afterCompile.tap('WatchRust', (compilation) => {
      compilation.contextDependencies.add(srcDir);
    });

    compiler.hooks.watchRun.tapPromise('BuildRust', async ({ modifiedFiles }) => {
      const rustChanged = [...(modifiedFiles ?? [])].some(
        (file) => file === srcDir || (file.startsWith(srcDir) && file.endsWith('.rs')),
      );
      if (!rustChanged) {
        return;
      }

      // Resolves at most once, however the build script terminates.
      await new Promise((resolve) => {
        const build = spawn('./build.sh', ['dev'], {
          stdio: 'inherit',
          cwd: import.meta.dirname,
        });

        build.on('error', (err) => {
          console.error('Failed to start build script:', err);
          resolve();
        });

        build.on('close', (code) => {
          if (code === 0) {
            console.log('Rust build successful.');
          } else {
            console.error(`Rust build failed with code ${code}`);
          }
          resolve();
        });
      });
    });
  }
}

export default (env, argv) => {
  const isProduction = argv.mode === 'production';

  const assetName = (name, ext) =>
    isProduction ? `assets/${name}-[contenthash]${ext}` : `assets/${name}${ext}`;

  return {
    entry: {
      'c6ol-client': './assets/entry.js',
    },
    output: {
      filename: assetName('[name]', '.js'),
      path: path.join(import.meta.dirname, 'dist'),
      clean: true,
      assetModuleFilename: assetName('[name]', '[ext]'),
    },
    module: {
      rules: [
        {
          test: /\.css$/i,
          use: [MiniCssExtractPlugin.loader, 'css-loader'],
        },
        {
          test: /\.wasm$/i,
          type: 'asset/resource',
        },
      ],
    },
    plugins: [
      new HtmlWebpackPlugin({
        template: 'index.html',
        publicPath,
        scriptLoading: 'module',
      }),
      new MiniCssExtractPlugin({
        filename: assetName('style', '.css'),
      }),
      new CopyWebpackPlugin({
        patterns: [
          {
            from: 'assets/manifest.json',
            to: 'assets/[name]-[contenthash][ext]',
            transform(content) {
              const manifest = JSON.parse(content.toString());
              manifest.icons[0].src = publicPath + iconAssetName();
              return JSON.stringify(manifest);
            },
          },
          {
            from: 'assets/icon.svg',
            to: iconAssetName,
          },
        ],
      }),
      new BuildRustPlugin(),
    ],
    experiments: {
      outputModule: true,
    },
    optimization: {
      minimizer: [
        '...',
        new CssMinimizerPlugin(),
      ],
    },
    performance: {
      maxAssetSize: 512000,
    },
    devServer: {
      port: 8080,
      proxy: [
        {
          context: ['/ws'],
          target: 'ws://localhost:8086',
          ws: true,
        },
      ],
      client: {
        webSocketURL: {
          pathname: '/webpack-ws',
        },
      },
      webSocketServer: {
        options: {
          path: '/webpack-ws',
        },
      },
    },
  };
};
