import path from 'path';
import HtmlWebpackPlugin from 'html-webpack-plugin';
import CopyWebpackPlugin from 'copy-webpack-plugin';
import MiniCssExtractPlugin from 'mini-css-extract-plugin';
import CssMinimizerPlugin from 'css-minimizer-webpack-plugin';
import crypto from 'crypto';
import fs from 'fs';
import { spawn } from 'child_process';

function getHash(content) {
  return crypto.createHash('md5').update(content).digest('hex').slice(0, 20);
}

export default {
  entry: {
    'c6ol-client': './assets/entry.js',
  },
  output: {
    filename: 'assets/[name]-[contenthash].js',
    path: path.join(import.meta.dirname, 'dist'),
    clean: true,
    library: {
      type: 'module',
    },
    assetModuleFilename: 'assets/[name]-[contenthash][ext]'
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
      template: "index.html",
      inject: false,
    }),
    new MiniCssExtractPlugin({
      filename: 'assets/style-[contenthash].css',
    }),
    new CopyWebpackPlugin({
      patterns: [
        {
          from: "assets/manifest.json",
          to: () => "assets/[name]-[contenthash][ext]",
          transform(content) {
            const manifest = JSON.parse(content.toString());
            const iconContent = fs.readFileSync('assets/icon.svg');
            const iconHash = getHash(iconContent);
            manifest.icons[0].src = `/assets/icon-${iconHash}.svg`;
            return JSON.stringify(manifest);
          }
        },
        {
          from: "assets/icon.svg",
          to: () => {
            const content = fs.readFileSync('assets/icon.svg');
            const hash = getHash(content);
            return `assets/icon-${hash}.svg`;
          }
        }
      ],
    }),
    {
      apply: (compiler) => {
        compiler.hooks.afterCompile.tap('WatchRust', (compilation) => {
          compilation.contextDependencies.add(path.resolve(import.meta.dirname, 'src'));
        });

        compiler.hooks.watchRun.tapAsync('BuildRust', (params, callback) => {
          const modifiedFiles = params.modifiedFiles;
          let rustChanged = false;
          const srcDir = path.resolve(import.meta.dirname, 'src');

          if (modifiedFiles) {
            for (const file of modifiedFiles) {
              if (file.startsWith(srcDir)) {
                if (file.endsWith('.rs') || file === srcDir) {
                  rustChanged = true;
                  break;
                }
              }
            }
          }

          if (rustChanged) {
            const build = spawn('./build.sh dev', [], { stdio: 'inherit', shell: true });

            build.on('error', (err) => {
              console.error('Failed to start build script:', err);
              callback();
            });

            build.on('close', (code) => {
              if (code === 0) {
                console.log('Rust build successful.');
              } else {
                console.error(`Rust build failed with code ${code}`);
              }
              callback();
            });
          } else {
            callback();
          }
        });
      }
    }
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
    maxAssetSize: 512000
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
