from PIL import Image
import sys
import os

def split_image(input_path, output_dir=None):
    """Split an image horizontally into 3 equal parts"""
    if output_dir is None:
        output_dir = os.path.dirname(input_path) or "."
    
    # Load image
    img = Image.open(input_path)
    width, height = img.size
    
    # Calculate part width
    part_width = width // 3
    
    # Split into 3 parts
    parts = [
        img.crop((0, 0, part_width, height)),
        img.crop((part_width, 0, part_width * 2, height)),
        img.crop((part_width * 2, 0, width, height))
    ]
    
    # Save parts
    base_name = os.path.splitext(os.path.basename(input_path))[0]
    names = ["icon_idle", "icon_recording", "icon_processing"]
    
    for i, (part, name) in enumerate(zip(parts, names)):
        output_path = os.path.join(output_dir, f"{name}.png")
        part.save(output_path)
        print(f"Saved: {output_path} ({part.size[0]}x{part.size[1]})")
    
    print("Done!")

if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("Usage: python split_icons.py <image_path>")
        sys.exit(1)
    
    input_path = sys.argv[1]
    split_image(input_path)
